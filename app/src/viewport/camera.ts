// The camera controller: every pointer and wheel input that moves the 3D, plan and section cameras.
// It replaces three's OrbitControls, whose orbit we had already overridden, so that pan and zoom can
// work on what is under the cursor too (decision 0044). A turn, a pan in perspective and a wheel tick
// each wait for their pick, so none starts about the wrong point.
//
// Our mouse layout, in 3D: left drag turns about the point under the cursor, right drag pans with the point grabbed
// staying under the cursor, middle drag dollies, and the wheel zooms towards the cursor. Plan and section: left or
// right drag pans, the wheel zooms about the cursor; neither turns. Spry's layout: middle or right drag pans, middle
// then left turns, and a left drag is the selection box's. In either, holding the turn key (Z) makes a left drag turn,
// in plan too (the view tilts out of plan into 3D), and holding the pan key (Space) makes it pan. The layout and keys
// are in viewControls.ts.
import * as THREE from 'three/webgpu';
import type { Vec3 } from '../bridge';
import { inView } from './pivot';
import { viewControls } from './viewControls';

const PIVOT_MARK_COLOUR = 0xffd23f;
/** How close a turn comes to looking straight up or down, in radians. */
const TURN_LIMIT = 0.01;
/** A wheel tick of 100 (one notch in most browsers) moves 5 % of the way to what is under the cursor. */
const ZOOM_PER_HUNDRED = 0.95;

/** A key typed into a text box or a list, where a letter isn't a shortcut. */
export function typing(target: EventTarget | null): boolean {
  return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement || (target instanceof HTMLElement && target.isContentEditable);
}

/** What the controller needs from the viewport. */
export interface CameraHost {
  readonly renderer: { readonly domElement: HTMLCanvasElement };
  /** The camera in use: the 3D one (perspective or orthographic), the plan one or the section one. What a move is measured by. */
  readonly camera: THREE.PerspectiveCamera | THREE.OrthographicCamera;
  /** The camera a move works on: `camera`, except in orthographic 3D, where it's the 3D camera the orthographic one copies. */
  readonly rig: THREE.PerspectiveCamera | THREE.OrthographicCamera;
  /** Puts `camera` where `rig` says, after a move (orthographic 3D); nothing otherwise. */
  syncCamera?(): void;
  /** Plan or section: a view that pans with the left button and does not turn. */
  readonly flatView: boolean;
  /** Plan, and not a section, which is never turned. */
  readonly planView: boolean;
  readonly sectionView: boolean;
  /** Leaves plan for 3D looking straight down on the same spot at the same scale, north up, to be turned from there. */
  tiltFromPlan(): void;
  /** What is drawn under a screen point, scene-local. */
  readonly pickPoint?: (clientX: number, clientY: number) => Promise<Vec3 | null>;
  readonly pivotMark: { show(point: THREE.Vector3, colour: number): void; hide(): void };
  /** Holds drawing for up to `ms` (0 lets it go): nothing moves while a turn or zoom waits for its pick. */
  holdFrames?(ms: number): void;
}

/** The longest a turn or zoom holds drawing while its pick comes back. */
const HOLD_FOR_PICK_MS = 250;

type Drag =
  /** `chord`: started by Spry's middle and left, so letting the left go pans again. */
  | { kind: 'turn'; pointer: number; pivot: THREE.Vector3; x: number; y: number; ready: boolean; latest: [number, number]; chord: boolean }
  /** `grabbed`: the point under the cursor at the press, which stays under it (null pans at the target's depth). */
  | { kind: 'pan'; pointer: number; grabbed: THREE.Vector3 | null; x: number; y: number; ready: boolean; latest: [number, number] }
  | { kind: 'dolly'; pointer: number; x: number; y: number; ndc: [number, number] };

export class CameraController {
  /** The point the camera looks at. Strings use its RL for the working plane. */
  readonly target = new THREE.Vector3();
  /** Off while something else owns the pointer, such as a vertex being dragged. */
  enabled = true;
  /** The turn or pan key is held: a left press is the camera's. Others that take a left press leave it to the camera. */
  get viewKeyHeld(): boolean {
    return this.turnKey || this.panKey;
  }
  private turnKey = false;
  private panKey = false;

  private drag: Drag | null = null;
  /** The last point a turn went about: a press over nothing turns about it again while it's in view (decision 0044). */
  private lastPivot: THREE.Vector3 | null = null;
  /** Wheel ticks waiting on a pick: their zoom scales multiply, and the latest cursor is used. */
  private wheel: { scale: number; x: number; y: number } | null = null;
  private readonly canvas: HTMLCanvasElement;

  constructor(private readonly host: CameraHost) {
    this.canvas = host.renderer.domElement;
    this.canvas.style.touchAction = 'none';
    this.canvas.addEventListener('pointerdown', this.pointerDown);
    this.canvas.addEventListener('pointermove', this.pointerMove);
    this.canvas.addEventListener('pointerup', this.pointerUp);
    this.canvas.addEventListener('pointercancel', this.pointerUp);
    this.canvas.addEventListener('wheel', this.onWheel, { passive: false });
    this.canvas.addEventListener('contextmenu', this.noMenu);
    window.addEventListener('keydown', this.keyDown);
    window.addEventListener('keyup', this.keyUp);
    window.addEventListener('blur', this.letGo);
  }

  /** Points the camera at the target; run every frame, and after moving either by hand. */
  update(): void {
    this.host.rig.lookAt(this.target);
    this.host.syncCamera?.();
  }

  dispose(): void {
    this.canvas.removeEventListener('pointerdown', this.pointerDown);
    this.canvas.removeEventListener('pointermove', this.pointerMove);
    this.canvas.removeEventListener('pointerup', this.pointerUp);
    this.canvas.removeEventListener('pointercancel', this.pointerUp);
    this.canvas.removeEventListener('wheel', this.onWheel);
    this.canvas.removeEventListener('contextmenu', this.noMenu);
    window.removeEventListener('keydown', this.keyDown);
    window.removeEventListener('keyup', this.keyUp);
    window.removeEventListener('blur', this.letGo);
  }

  private noMenu = (e: Event): void => e.preventDefault();

  private keyDown = (e: KeyboardEvent): void => {
    if (e.ctrlKey || e.metaKey || e.altKey || typing(e.target)) return;
    const key = e.key.toLowerCase();
    const { turn, pan } = viewControls().keys;
    if (key !== turn && key !== pan) return;
    // Space would also press a button that has the focus, or scroll: it's the view's while held.
    if (key === ' ') e.preventDefault();
    this.hold(key === turn ? 'turn' : 'pan', true);
  };

  private keyUp = (e: KeyboardEvent): void => {
    const key = e.key.toLowerCase();
    const { turn, pan } = viewControls().keys;
    if (key === ' ' && (key === turn || key === pan) && !typing(e.target)) e.preventDefault();
    if (key === turn) this.hold('turn', false);
    if (key === pan) this.hold('pan', false);
  };

  /** The window lost the focus with a key down: its key-up never comes. */
  private letGo = (): void => {
    this.hold('turn', false);
    this.hold('pan', false);
  };

  private hold(which: 'turn' | 'pan', held: boolean): void {
    if (which === 'turn') this.turnKey = held;
    else this.panKey = held;
    this.canvas.style.cursor = this.turnKey || this.panKey ? 'grab' : '';
  }

  /**
   * What a press does, by the mouse layout (decision 0044, section 7). Either layout: the turn key and a left drag turn
   * (out of plan too), the pan key and a left drag pan. Ours: left turns (pans in plan and section), right pans, middle
   * dollies. Spry's: middle or right pans, and left is the click's or the selection box's; middle and then left turns.
   */
  private kindOf(button: number): Drag['kind'] | null {
    const flat = this.host.flatView;
    if (button === 0 && this.panKey) return 'pan';
    if (button === 0 && this.turnKey) return flat ? 'pan' : 'turn';
    if (viewControls().layout === 'spry') return button === 1 || button === 2 ? 'pan' : null;
    return button === 1 ? 'dolly' : button === 2 || (button === 0 && flat) ? 'pan' : button === 0 ? 'turn' : null;
  }

  private pointerDown = (e: PointerEvent): void => {
    if (!this.enabled || this.drag) return;
    // Turning from plan: into 3D first, looking straight down, so the pick and the turn are in the view turned.
    if (e.button === 0 && this.turnKey && !this.panKey && this.host.planView && !this.host.sectionView) this.host.tiltFromPlan();
    const kind = this.kindOf(e.button);
    if (!kind) return;
    // A middle press would start the browser's autoscroll.
    if (e.button === 1) e.preventDefault();
    try {
      this.canvas.setPointerCapture(e.pointerId);
    } catch {
      // A pointer the browser no longer knows: the drag still works while the cursor stays on the canvas.
    }
    if (kind === 'turn') return this.startTurn(e.pointerId, e.clientX, e.clientY);
    if (kind === 'pan') return this.startPan(e.pointerId, e.clientX, e.clientY);
    this.drag = { kind, pointer: e.pointerId, x: e.clientX, y: e.clientY, ndc: this.ndc(e.clientX, e.clientY) };
  };

  private startTurn(pointer: number, x: number, y: number, chord = false): void {
    const turn: Drag = { kind: 'turn', pointer, pivot: this.target.clone(), x, y, ready: false, latest: [x, y], chord };
    this.drag = turn;
    // Over empty space it turns about the nearest thing to the cursor (the pick's fan of rays), else the last point
    // turned about while it's in view, else the view's centre. The turn waits for its pick, so the first move never
    // turns about the wrong point; drawing holds meanwhile, since nothing moves, so the pick comes back at
    // once even on a scene that keeps the page busy drawing. The gold mark shows where it will turn as soon
    // as the pick lands, before the first move.
    this.host.holdFrames?.(HOLD_FOR_PICK_MS);
    const pick = this.host.pickPoint?.(x, y) ?? Promise.resolve(null);
    void pick.then((hit) => {
      this.host.holdFrames?.(0);
      if (hit) turn.pivot.set(...hit);
      else if (this.lastPivot && inView(this.lastPivot.clone().project(this.host.camera))) turn.pivot.copy(this.lastPivot);
      this.lastPivot = turn.pivot.clone();
      turn.ready = true;
      if (this.drag !== turn) return;
      this.host.pivotMark.show(turn.pivot, PIVOT_MARK_COLOUR);
      const [lx, ly] = turn.latest;
      if (lx !== turn.x || ly !== turn.y) this.turnTo(turn, lx, ly);
    });
  }

  private startPan(pointer: number, x: number, y: number): void {
    const pan: Drag = { kind: 'pan', pointer, grabbed: null, x, y, ready: true, latest: [x, y] };
    this.drag = pan;
    // In perspective a pan sticks to the point grabbed (0044): it pans at that point's depth, so it needs the pick
    // first, waited for as a turn's is. In plan and section, and in orthographic 3D, a pixel is the same at every
    // depth: no pick.
    if (!(this.host.camera instanceof THREE.PerspectiveCamera) || !this.host.pickPoint) return;
    pan.ready = false;
    this.host.holdFrames?.(HOLD_FOR_PICK_MS);
    void this.host.pickPoint(x, y).then((hit) => {
      this.host.holdFrames?.(0);
      if (hit) pan.grabbed = new THREE.Vector3(...hit);
      pan.ready = true;
      if (this.drag !== pan) return;
      const [lx, ly] = pan.latest;
      if (lx !== pan.x || ly !== pan.y) this.panTo(pan, lx, ly);
    });
  }

  /**
   * Spry's chord: with the middle button held (a pan), pressing the left as well turns about the point under the
   * cursor, out of plan too; letting the left go pans again. A browser sends a second button's press as a move.
   */
  private chord(e: PointerEvent, drag: Drag): boolean {
    if (e.button < 0 || viewControls().layout !== 'spry') return false;
    const [middle, left] = [(e.buttons & 4) !== 0, (e.buttons & 1) !== 0];
    if (drag.kind === 'pan' && middle && left && !this.host.sectionView) {
      if (this.host.planView) this.host.tiltFromPlan();
      this.startTurn(drag.pointer, e.clientX, e.clientY, true);
      return true;
    }
    if (drag.kind === 'turn' && drag.chord && middle && !left) {
      this.host.pivotMark.hide();
      this.startPan(drag.pointer, e.clientX, e.clientY);
      return true;
    }
    return false;
  }

  private pointerMove = (e: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || e.pointerId !== drag.pointer || !this.enabled) return;
    if (this.chord(e, drag)) return;
    if (drag.kind === 'turn') {
      drag.latest = [e.clientX, e.clientY];
      if (drag.ready) this.turnTo(drag, e.clientX, e.clientY);
      return;
    }
    if (drag.kind === 'pan') {
      drag.latest = [e.clientX, e.clientY];
      if (drag.ready) this.panTo(drag, e.clientX, e.clientY);
      return;
    }
    const dy = e.clientY - drag.y;
    drag.x = e.clientX;
    drag.y = e.clientY;
    if (dy !== 0) this.zoomBy(zoomScale(dy), drag.ndc);
  };

  private panTo(pan: Extract<Drag, { kind: 'pan' }>, x: number, y: number): void {
    this.pan(x - pan.x, y - pan.y, pan.grabbed);
    pan.x = x;
    pan.y = y;
  }

  private pointerUp = (e: PointerEvent): void => {
    if (!this.drag || e.pointerId !== this.drag.pointer) return;
    if (this.drag.kind === 'turn') this.host.pivotMark.hide();
    this.drag = null;
    try {
      this.canvas.releasePointerCapture(e.pointerId);
    } catch {
      // Already released.
    }
  };

  private turnTo(turn: Extract<Drag, { kind: 'turn' }>, x: number, y: number): void {
    const fullTurn = (2 * Math.PI) / (this.canvas.clientHeight || 1);
    this.turnAbout(turn.pivot, -fullTurn * (x - turn.x), fullTurn * (y - turn.y));
    this.host.pivotMark.show(turn.pivot, PIVOT_MARK_COLOUR);
    turn.x = x;
    turn.y = y;
  }

  /**
   * Swings the camera and its target together about `pivot`: `yaw` about the vertical, `raise` up
   * over the top (radians), stopping short of straight up or down. Moving both is what keeps the
   * pivot where it is on screen; the camera does not jump to look at it.
   */
  private turnAbout(pivot: THREE.Vector3, yaw: number, raise: number): void {
    const camera = this.host.rig;
    const forward = camera.getWorldDirection(new THREE.Vector3());
    const right = forward.clone().cross(camera.up).normalize();
    const fromAbove = forward.angleTo(camera.up.clone().negate());
    const raised = Math.min(raise, fromAbove - TURN_LIMIT);
    const lowered = Math.max(raised, fromAbove - (Math.PI - TURN_LIMIT));
    const swing = new THREE.Quaternion().setFromAxisAngle(camera.up, yaw).multiply(new THREE.Quaternion().setFromAxisAngle(right, -lowered));
    for (const point of [camera.position, this.target]) {
      point.sub(pivot).applyQuaternion(swing).add(pivot);
    }
    this.update();
  }

  /**
   * Slides the camera and target across the screen by a drag of `dx`, `dy` pixels. In perspective a pixel
   * is measured at the depth of `grabbed` (the target's when there is none): the camera only slides, so
   * that depth never changes and the point grabbed stays exactly under the cursor, near or far (0044).
   * In plan, section and orthographic 3D a pixel is the same everywhere.
   */
  private pan(dx: number, dy: number, grabbed: THREE.Vector3 | null = null): void {
    const camera = this.host.camera;
    const rig = this.host.rig;
    rig.updateMatrix();
    const right = new THREE.Vector3().setFromMatrixColumn(rig.matrix, 0);
    const up = new THREE.Vector3().setFromMatrixColumn(rig.matrix, 1);
    let across: number;
    let along: number;
    if (camera instanceof THREE.PerspectiveCamera) {
      const forward = camera.getWorldDirection(new THREE.Vector3());
      const grabbedDepth = grabbed ? grabbed.clone().sub(camera.position).dot(forward) : 0;
      const depth = grabbedDepth > 0 ? grabbedDepth : this.target.clone().sub(camera.position).dot(forward);
      const metresPerPixel = (2 * depth * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) / (this.canvas.clientHeight || 1);
      across = dx * metresPerPixel;
      along = dy * metresPerPixel;
    } else {
      across = (dx * (camera.right - camera.left)) / camera.zoom / (this.canvas.clientWidth || 1);
      along = (dy * (camera.top - camera.bottom)) / camera.zoom / (this.canvas.clientHeight || 1);
    }
    const offset = right.multiplyScalar(-across).add(up.multiplyScalar(along));
    rig.position.add(offset);
    this.target.add(offset);
    this.update();
  }

  private onWheel = (e: WheelEvent): void => {
    if (!this.enabled || this.drag) return;
    e.preventDefault();
    const delta = wheelDelta(e);
    if (delta === 0) return;
    const scale = zoomScale(delta);
    // With no perspective a wheel scales the view about the cursor: no depth to find.
    if (!(this.host.camera instanceof THREE.PerspectiveCamera) || !this.host.pickPoint) {
      this.zoomBy(scale, this.ndc(e.clientX, e.clientY));
      return;
    }
    if (this.wheel) {
      // A pick is in flight: this tick goes with it.
      this.wheel.scale *= scale;
      this.wheel.x = e.clientX;
      this.wheel.y = e.clientY;
      return;
    }
    const wheel = { scale, x: e.clientX, y: e.clientY };
    this.wheel = wheel;
    this.host.holdFrames?.(HOLD_FOR_PICK_MS);
    void this.host.pickPoint(e.clientX, e.clientY).then((hit) => {
      this.host.holdFrames?.(0);
      this.wheel = null;
      if (hit) this.settleDepth(new THREE.Vector3(...hit));
      this.zoomBy(wheel.scale, this.ndc(wheel.x, wheel.y));
    });
  };

  /**
   * Puts the target at the depth of `point`, along the line of sight so the view does not move:
   * zooming then slows as it nears what is under the cursor, and stops short of it.
   */
  private settleDepth(point: THREE.Vector3): void {
    const camera = this.host.camera;
    const forward = camera.getWorldDirection(new THREE.Vector3());
    const depth = point.clone().sub(camera.position).dot(forward);
    if (depth > 0) this.target.copy(camera.position).addScaledVector(forward, depth);
  }

  /**
   * Zooms by `scale` towards the screen point `ndc` (below 1 is in). In perspective the camera moves along
   * the ray through that point by the share of its distance to the target; with no perspective (plan,
   * section, orthographic 3D) the view scales about it, so what is under the cursor stays there.
   */
  private zoomBy(scale: number, [nx, ny]: [number, number]): void {
    const camera = this.host.camera;
    const rig = this.host.rig;
    const distance = camera.position.distanceTo(this.target);
    if (rig !== camera) {
      // Orthographic 3D: its scale is the 3D camera's distance to the target, so that closes in along the line of
      // sight, then both slide across the screen so what is under the cursor stays.
      const before = new THREE.Vector3(nx, ny, 0).unproject(camera);
      rig.position.sub(this.target).multiplyScalar(scale).add(this.target);
      this.host.syncCamera?.();
      const after = new THREE.Vector3(nx, ny, 0).unproject(camera);
      const forward = rig.getWorldDirection(new THREE.Vector3());
      const slide = before.sub(after);
      slide.addScaledVector(forward, -slide.dot(forward));
      rig.position.add(slide);
      this.target.add(slide);
    } else if (camera instanceof THREE.PerspectiveCamera) {
      const along = new THREE.Vector3(nx, ny, 0.5).unproject(camera).sub(camera.position).normalize();
      const nearer = distance * scale;
      camera.position.addScaledVector(along, distance - nearer);
      camera.updateMatrixWorld();
      this.target.copy(camera.position).addScaledVector(camera.getWorldDirection(new THREE.Vector3()), nearer);
    } else {
      const before = new THREE.Vector3(nx, ny, 0).unproject(camera);
      camera.zoom /= scale;
      camera.updateProjectionMatrix();
      const after = new THREE.Vector3(nx, ny, 0).unproject(camera);
      camera.position.add(before.sub(after));
      camera.updateMatrixWorld();
      this.target.copy(camera.position).addScaledVector(camera.getWorldDirection(new THREE.Vector3()), distance);
    }
    this.update();
  }

  private ndc(clientX: number, clientY: number): [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    return [((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1];
  }
}

/** A wheel's delta in pixels, whatever unit the browser reports it in. A pinch counts ten times. */
function wheelDelta(e: WheelEvent): number {
  const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 100 : 1;
  return e.deltaY * unit * (e.ctrlKey ? 10 : 1);
}

/**
 * The share of the distance left after a wheel delta or a dolly drag of `delta` pixels: below 1 in
 * (wheel up, or drag up), above 1 out. A dragged pixel counts as a wheel delta of one.
 */
function zoomScale(delta: number): number {
  const step = Math.pow(ZOOM_PER_HUNDRED, Math.abs(delta) * 0.01);
  return delta < 0 ? step : 1 / step;
}
