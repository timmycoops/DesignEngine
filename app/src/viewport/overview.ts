// The section's overview: a small plan map in the corner showing where the section is and which way it
// looks, with the step increments either side. Dragging on it moves the section square to itself.
// The map is a picture taken of the plan view when a section opens, so it costs nothing while stepping;
// it is taken again if the section walks off it.
import type { PlanPoint } from './section';

/** The map's size on screen (CSS px). */
const SIZE = 240;
/** How much of the map's half width the section may wander before it is taken again. */
const KEEP_IN = 0.85;
/** Step lines drawn either side of the section. */
const STEP_LINES = 4;

const SECTION = '#ffd23f';
const STEPS = 'rgba(255, 210, 63, 0.3)';
const TEXT = '#ffffff';
const SHADE = 'rgba(10, 14, 20, 0.7)';

export class SectionOverview {
  private canvas = Object.assign(document.createElement('canvas'), { className: 'section-overview', hidden: true });
  private map = document.createElement('canvas');
  private pixels: number;
  /** What the map covers, scene-local. */
  private frame = { x: 0, y: 0, half: 1 };
  private a: PlanPoint = [0, 0];
  private b: PlanPoint = [1, 0];
  private dragging = false;
  /** What was last drawn, to draw again when a drag ends. */
  private last = { step: 0, offset: 0 };
  /** A drag or click on the map: how far the pointer is off the section, to its left (m). */
  onPointAt?: (offset: number) => void;

  constructor(
    host: HTMLElement,
    /** Takes the plan picture onto a 2D canvas: `Viewport.planPicture` with what else to show. */
    private picture: (target: CanvasRenderingContext2D, x: number, y: number, half: number) => void,
  ) {
    this.pixels = Math.round(SIZE * window.devicePixelRatio);
    for (const canvas of [this.canvas, this.map]) Object.assign(canvas, { width: this.pixels, height: this.pixels });
    host.append(this.canvas);
    this.canvas.addEventListener('pointerdown', (e) => {
      this.dragging = true;
      this.canvas.setPointerCapture?.(e.pointerId);
      this.pointAt(e);
    });
    this.canvas.addEventListener('pointermove', (e) => {
      if (this.dragging) this.pointAt(e);
    });
    for (const end of ['pointerup', 'pointercancel']) {
      this.canvas.addEventListener(end, () => {
        this.dragging = false;
        this.draw(this.a, this.b, this.last.step, this.last.offset);
      });
    }
  }

  get canvasElement(): HTMLCanvasElement {
    return this.canvas;
  }

  show(): void {
    this.canvas.hidden = false;
  }

  hide(): void {
    this.canvas.hidden = true;
  }

  /** Takes a new picture of the plan around the section. */
  retake(a: PlanPoint, b: PlanPoint): void {
    const half = Math.max(Math.hypot(b[0] - a[0], b[1] - a[1]) * 0.75, 400);
    this.frame = { x: (a[0] + b[0]) / 2, y: (a[1] + b[1]) / 2, half };
    const context = this.map.getContext('2d');
    if (context) this.picture(context, this.frame.x, this.frame.y, half);
  }

  /** Draws the section on the map, scene-local, with `step` metres between step lines. */
  draw(a: PlanPoint, b: PlanPoint, step: number, offset: number): void {
    [this.a, this.b] = [a, b];
    this.last = { step, offset };
    const middle: PlanPoint = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
    const { x, y, half } = this.frame;
    // Never under a drag: the map would slide away beneath the pointer.
    if (!this.dragging && Math.max(Math.abs(middle[0] - x), Math.abs(middle[1] - y)) > half * KEEP_IN) this.retake(a, b);
    const context = this.canvas.getContext('2d');
    if (!context) return;
    const n = this.pixels;
    context.clearRect(0, 0, n, n);
    context.drawImage(this.map, 0, 0);

    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const [dx, dy] = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
    const scale = this.pixels / 100;
    const line = (from: PlanPoint, to: PlanPoint, colour: string, width: number) => {
      context.strokeStyle = colour;
      context.lineWidth = width * scale;
      context.beginPath();
      context.moveTo(...this.toMap(from));
      context.lineTo(...this.toMap(to));
      context.stroke();
    };
    // Step lines, when they stand far enough apart to tell one from the next.
    if ((step / (2 * this.frame.half)) * n > 2 * scale) {
      for (let k = -STEP_LINES; k <= STEP_LINES; k++) {
        const [ox, oy] = [-dy * step * k, dx * step * k];
        if (k !== 0) line([a[0] + ox, a[1] + oy], [b[0] + ox, b[1] + oy], STEPS, 0.3);
      }
    }
    line(a, b, SECTION, 0.8);
    // Which way the section looks, from its middle.
    const reach = (this.frame.half * 16) / 100;
    line(middle, [middle[0] - dy * reach, middle[1] + dx * reach], SECTION, 0.8);

    const label = (text: string, at: PlanPoint) => {
      context.font = `${5 * scale}px system-ui, sans-serif`;
      context.textAlign = 'center';
      context.textBaseline = 'middle';
      const [px, py] = this.toMap(at);
      const [hx, hy] = [Math.min(Math.max(px, 5 * scale), n - 5 * scale), Math.min(Math.max(py, 5 * scale), n - 5 * scale)];
      context.fillStyle = SHADE;
      context.beginPath();
      context.arc(hx, hy, 3.4 * scale, 0, 2 * Math.PI);
      context.fill();
      context.fillStyle = TEXT;
      context.fillText(text, hx, hy);
    };
    label('A', a);
    label('B', b);
    const text = offset === 0 ? 'At the start' : `${offset > 0 ? '+' : '−'}${Math.abs(offset).toLocaleString()} m`;
    context.font = `${4.4 * scale}px system-ui, sans-serif`;
    context.textAlign = 'left';
    context.textBaseline = 'top';
    context.fillStyle = SHADE;
    context.fillRect(0, 0, context.measureText(text).width + 3 * scale, 7 * scale);
    context.fillStyle = TEXT;
    context.fillText(text, 1.5 * scale, 1.5 * scale);
  }

  private toMap([x, y]: PlanPoint): [number, number] {
    const { x: cx, y: cy, half } = this.frame;
    const n = this.pixels;
    return [n / 2 + ((x - cx) / half) * (n / 2), n / 2 - ((y - cy) / half) * (n / 2)];
  }

  private pointAt(e: PointerEvent): void {
    const box = this.canvas.getBoundingClientRect();
    const { x: cx, y: cy, half } = this.frame;
    const [x, y] = [cx + (((e.clientX - box.left) / box.width) * 2 - 1) * half, cy + (1 - ((e.clientY - box.top) / box.height) * 2) * half];
    const [a, b] = [this.a, this.b];
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    this.onPointAt?.(((b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0])) / length);
  }
}
