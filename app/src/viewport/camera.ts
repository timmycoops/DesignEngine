// The camera controller: every pointer and wheel input that moves the 3D, plan and section cameras.
// It replaces three's OrbitControls, whose orbit we had already overridden, so that pan and zoom can
// work on what is under the cursor too (decision 0044). This first version handles as OrbitControls
// did, with one change: a turn or a wheel tick waits for its pick, so it never starts about the wrong
// point.
//
// 3D: left drag turns about the point under the cursor, right drag pans, middle drag dollies, and the
// wheel zooms towards the cursor. Plan and section: left or right drag pans, the wheel zooms about the
// cursor; neither turns.
import * as THREE from 'three/webgpu';
import type { Vec3 } from '../bridge';

const PIVOT_MARK_COLOUR = 0xffd23f;
/** How close a turn comes to looking straight up or down, in radians. */
const TURN_LIMIT = 0.01;
/** A wheel tick of 100 (one notch in most browsers) moves 5 % of the way to what is under the cursor. */
const ZOOM_PER_HUNDRED = 0.95;

/** What the controller needs from the viewport. */
export interface CameraHost {
  readonly renderer: { readonly domElement: HTMLCanvasElement };
  /** The camera in use: the 3D one, the plan one or the section one. */
  readonly camera: THREE.PerspectiveCamera | THREE.OrthographicCamera;
  /** Plan or section: a view that pans with the left button and does not turn. */
  readonly flatView: boolean;
  /** What is drawn under a screen point, scene-local. */
  readonly pickPoint?: (clientX: number, clientY: number) => Promise<Vec3 | null>;
  readonly pivotMark: { show(point: THREE.Vector3, colour: number): void; hide(): void };
}

type Drag =
  | { kind: 'turn'; pointer: number; pivot: THREE.Vector3; x: number; y: number; ready: boolean; latest: [number, number] }
  | { kind: 'pan' | 'dolly'; pointer: number; x: number; y: number; ndc: [number, number] };

export class CameraController {
  /** The point the camera looks at. Strings use its RL for the working plane. */
  readonly target = new THREE.Vector3();
  /** Off while something else owns the pointer, such as a vertex being dragged. */
  enabled = true;

  private drag: Drag | null = null;
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
  }

  /** Points the camera at the target; run every frame, and after moving either by hand. */
  update(): void {
    this.host.camera.lookAt(this.target);
  }

  dispose(): void {
    this.canvas.removeEventListener('pointerdown', this.pointerDown);
    this.canvas.removeEventListener('pointermove', this.pointerMove);
    this.canvas.removeEventListener('pointerup', this.pointerUp);
    this.canvas.removeEventListener('pointercancel', this.pointerUp);
    this.canvas.removeEventListener('wheel', this.onWheel);
    this.canvas.removeEventListener('contextmenu', this.noMenu);
  }

  private noMenu = (e: Event): void => e.preventDefault();

  private pointerDown = (e: PointerEvent): void => {
    if (!this.enabled || this.drag) return;
    const flat = this.host.flatView;
    const kind = e.button === 1 ? 'dolly' : e.button === 2 || (e.button === 0 && flat) ? 'pan' : e.button === 0 ? 'turn' : null;
    if (!kind) return;
    try {
      this.canvas.setPointerCapture(e.pointerId);
    } catch {
      // A pointer the browser no longer knows: the drag still works while the cursor stays on the canvas.
    }
    if (kind === 'turn') {
      const turn: Drag = { kind, pointer: e.pointerId, pivot: this.target.clone(), x: e.clientX, y: e.clientY, ready: false, latest: [e.clientX, e.clientY] };
      this.drag = turn;
      // Over empty space it keeps the view's centre. The pick takes about a millisecond, and the turn
      // waits for it, so the first move never turns about the wrong point.
      const pick = this.host.pickPoint?.(e.clientX, e.clientY) ?? Promise.resolve(null);
      void pick.then((hit) => {
        if (hit) turn.pivot.set(...hit);
        turn.ready = true;
        const [x, y] = turn.latest;
        if (this.drag === turn && (x !== turn.x || y !== turn.y)) this.turnTo(turn, x, y);
      });
      return;
    }
    this.drag = { kind, pointer: e.pointerId, x: e.clientX, y: e.clientY, ndc: this.ndc(e.clientX, e.clientY) };
  };

  private pointerMove = (e: PointerEvent): void => {
    const drag = this.drag;
    if (!drag || e.pointerId !== drag.pointer || !this.enabled) return;
    if (drag.kind === 'turn') {
      drag.latest = [e.clientX, e.clientY];
      if (drag.ready) this.turnTo(drag, e.clientX, e.clientY);
      return;
    }
    const [dx, dy] = [e.clientX - drag.x, e.clientY - drag.y];
    drag.x = e.clientX;
    drag.y = e.clientY;
    if (drag.kind === 'pan') this.pan(dx, dy);
    else if (dy !== 0) this.zoomBy(zoomScale(dy), drag.ndc);
  };

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
    const camera = this.host.camera;
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
   * Slides the camera and target across the screen by a drag of `dx`, `dy` pixels. In 3D a pixel is
   * measured at the target's depth; in plan and section a pixel is the same everywhere.
   */
  private pan(dx: number, dy: number): void {
    const camera = this.host.camera;
    camera.updateMatrix();
    const right = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 0);
    const up = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 1);
    let across: number;
    let along: number;
    if (camera instanceof THREE.PerspectiveCamera) {
      const metresPerPixel = (2 * camera.position.distanceTo(this.target) * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2))) / (this.canvas.clientHeight || 1);
      across = dx * metresPerPixel;
      along = dy * metresPerPixel;
    } else {
      across = (dx * (camera.right - camera.left)) / camera.zoom / (this.canvas.clientWidth || 1);
      along = (dy * (camera.top - camera.bottom)) / camera.zoom / (this.canvas.clientHeight || 1);
    }
    const offset = right.multiplyScalar(-across).add(up.multiplyScalar(along));
    camera.position.add(offset);
    this.target.add(offset);
    this.update();
  }

  private onWheel = (e: WheelEvent): void => {
    if (!this.enabled || this.drag) return;
    e.preventDefault();
    const delta = wheelDelta(e);
    if (delta === 0) return;
    const scale = zoomScale(delta);
    if (this.host.flatView || !this.host.pickPoint) {
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
    void this.host.pickPoint(e.clientX, e.clientY).then((hit) => {
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
   * Zooms by `scale` towards the screen point `ndc` (below 1 is in). In 3D the camera moves along
   * the ray through that point by the share of its distance to the target; in plan and section the
   * view scales about it, so what is under the cursor stays there.
   */
  private zoomBy(scale: number, [nx, ny]: [number, number]): void {
    const camera = this.host.camera;
    const distance = camera.position.distanceTo(this.target);
    if (camera instanceof THREE.PerspectiveCamera) {
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
