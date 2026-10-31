// The section's overview: a small plan map in the corner showing where the section is and which way it
// looks, with the step increments either side. Dragging on it moves the section square to itself.
// The map slides over a picture of the plan three times its width, so it follows the section without
// drawing the scene; the picture is taken when a section opens and again when the map nears its edge.
import type { PlanPoint } from './section';

/** The map's size on screen (CSS px). */
const SIZE = 240;
/** How many times wider than the map its picture is. */
const PICTURE = 3;
/** How far from the map's centre the section's middle may go before it slides, as a share of its half width. */
const KEEP_NEAR = 0.25;
/** How far from the map's centre a drag starts to carry the map along, as a share of its half width. */
const CARRY_FROM = 0.8;
/** How far the map slides each frame of a drag, as a share of how far the pointer is past that. */
const CARRY = 0.08;
/** How much of the way back round the section the map eases each frame after a drag. */
const EASE = 0.25;
/** Step lines drawn either side of the section. */
const STEP_LINES = 4;

const SECTION = '#ffd23f';
const STEPS = 'rgba(255, 210, 63, 0.3)';
const TEXT = '#ffffff';
const SHADE = 'rgba(10, 14, 20, 0.7)';

export class SectionOverview {
  private canvas = Object.assign(document.createElement('canvas'), { className: 'section-overview', hidden: true });
  /** The picture of the plan the map slides over. */
  private plan = document.createElement('canvas');
  private pixels: number;
  /** What the map shows, scene-local: `half` metres each way from its centre. */
  private view = { x: 0, y: 0, half: 1 };
  /** Where the picture is centred, scene-local. */
  private pictured = { x: 0, y: 0 };
  private a: PlanPoint = [0, 0];
  private b: PlanPoint = [1, 0];
  /** The pointer during a drag, from -1 to 1 each way from the map's centre. */
  private held: [number, number] | null = null;
  /** Whether a drag, or the map easing back after one, has charge of the map. */
  private carrying = false;
  /** The step lines and offset, to paint again when only the map has moved. */
  private last = { step: 0, offset: 0 };
  /** A drag or click on the map: how far the pointer is off the section, to its left (m). */
  onPointAt?: (offset: number) => void;
  /** How long the last picture took (ms). */
  lastPictureMs = 0;

  constructor(
    host: HTMLElement,
    /** Takes the plan picture onto a 2D canvas: `Viewport.planPicture` with what else to show. */
    private picture: (target: CanvasRenderingContext2D, x: number, y: number, half: number) => void,
  ) {
    this.pixels = Math.round(SIZE * window.devicePixelRatio);
    Object.assign(this.canvas, { width: this.pixels, height: this.pixels });
    Object.assign(this.plan, { width: this.pixels * PICTURE, height: this.pixels * PICTURE });
    host.append(this.canvas);
    this.canvas.addEventListener('pointerdown', (e) => {
      this.canvas.setPointerCapture?.(e.pointerId);
      this.hold(e);
      if (!this.carrying) {
        this.carrying = true;
        this.carry();
      }
    });
    this.canvas.addEventListener('pointermove', (e) => {
      if (this.held) this.hold(e);
    });
    for (const end of ['pointerup', 'pointercancel']) {
      this.canvas.addEventListener(end, () => {
        this.held = null;
      });
    }
  }

  get canvasElement(): HTMLCanvasElement {
    return this.canvas;
  }

  /** Whether both ends of the section are on the map. */
  get sectionOnMap(): boolean {
    return [this.a, this.b].every(([x, y]) => Math.max(Math.abs(x - this.view.x), Math.abs(y - this.view.y)) <= this.view.half);
  }

  show(): void {
    this.canvas.hidden = false;
  }

  hide(): void {
    this.canvas.hidden = true;
  }

  /** Centres the map on a new section and takes its picture. */
  frameOn(a: PlanPoint, b: PlanPoint): void {
    this.view = { x: (a[0] + b[0]) / 2, y: (a[1] + b[1]) / 2, half: Math.max(Math.hypot(b[0] - a[0], b[1] - a[1]) * 0.75, 400) };
    this.takePicture();
  }

  /** Draws the section on the map, scene-local, with `step` metres between step lines. */
  draw(a: PlanPoint, b: PlanPoint, step: number, offset: number): void {
    [this.a, this.b] = [a, b];
    this.last = { step, offset };
    if (!this.held) {
      this.carrying = false;
      [this.view.x, this.view.y] = this.nearSection();
    }
    this.paint();
  }

  private paint(): void {
    const [a, b] = [this.a, this.b];
    const { step, offset } = this.last;
    const { x, y, half } = this.view;
    if (Math.max(Math.abs(x - this.pictured.x), Math.abs(y - this.pictured.y)) > (PICTURE - 1) * half) this.takePicture();
    const context = this.canvas.getContext('2d');
    if (!context) return;
    const n = this.pixels;
    const perMetre = n / (2 * half);
    context.clearRect(0, 0, n, n);
    context.drawImage(this.plan, (this.pictured.x - x) * perMetre - n, (y - this.pictured.y) * perMetre - n);

    const [nx, ny] = this.normal();
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
    if (step * perMetre > 2 * scale) {
      for (let k = -STEP_LINES; k <= STEP_LINES; k++) {
        const [ox, oy] = [nx * step * k, ny * step * k];
        if (k !== 0) line([a[0] + ox, a[1] + oy], [b[0] + ox, b[1] + oy], STEPS, 0.3);
      }
    }
    line(a, b, SECTION, 0.8);
    // Which way the section looks, from its middle.
    const middle: PlanPoint = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
    const reach = (half * 16) / 100;
    line(middle, [middle[0] + nx * reach, middle[1] + ny * reach], SECTION, 0.8);

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

  private takePicture(): void {
    const { x, y, half } = this.view;
    const context = this.plan.getContext('2d');
    const started = performance.now();
    if (context) this.picture(context, x, y, half * PICTURE);
    this.lastPictureMs = performance.now() - started;
    this.pictured = { x, y };
  }

  /** Where the map's centre goes to keep the section near it, with the plan ahead in view. */
  private nearSection(): PlanPoint {
    const reach = this.view.half * KEEP_NEAR;
    const within = (centre: number, middle: number) => Math.min(Math.max(centre, middle - reach), middle + reach);
    return [within(this.view.x, (this.a[0] + this.b[0]) / 2), within(this.view.y, (this.a[1] + this.b[1]) / 2)];
  }

  /** The way the section looks, a unit vector in plan, to its left. */
  private normal(): PlanPoint {
    const [a, b] = [this.a, this.b];
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    return [-(b[1] - a[1]) / length, (b[0] - a[0]) / length];
  }

  private hold(e: PointerEvent): void {
    const box = this.canvas.getBoundingClientRect();
    const across = (at: number) => Math.min(Math.max(at * 2 - 1, -1), 1);
    this.held = [across((e.clientX - box.left) / box.width), across(1 - (e.clientY - box.top) / box.height)];
  }

  /**
   * Follows a drag, a frame at a time. The section goes to the pointer, and near the map's edge the map
   * slides on as well, the faster the closer to the edge, so a drag can run past what is in view. When
   * it ends the map eases back round the section.
   */
  private carry = (): void => {
    if (!this.carrying) return;
    if (this.held) {
      const [px, py] = this.held;
      const [nx, ny] = this.normal();
      const past = (at: number) => Math.sign(at) * Math.max(Math.abs(at) - CARRY_FROM, 0);
      const slide = (past(px) * nx + past(py) * ny) * this.view.half * CARRY;
      this.view.x += nx * slide;
      this.view.y += ny * slide;
      const { x, y, half } = this.view;
      this.onPointAt?.((x + px * half - this.a[0]) * nx + (y + py * half - this.a[1]) * ny);
    } else {
      const [x, y] = this.nearSection();
      this.view.x += (x - this.view.x) * EASE;
      this.view.y += (y - this.view.y) * EASE;
      this.carrying = Math.hypot(x - this.view.x, y - this.view.y) > this.view.half / 200;
    }
    this.paint();
    if (this.carrying) requestAnimationFrame(this.carry);
  };

  private toMap([x, y]: PlanPoint): [number, number] {
    const { x: cx, y: cy, half } = this.view;
    const n = this.pixels;
    return [n / 2 + ((x - cx) / half) * (n / 2), n / 2 - ((y - cy) / half) * (n / 2)];
  }
}
