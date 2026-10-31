// Full-window three.js viewport on WebGPURenderer (falls back to WebGL2 by itself).
// The scene is in local coordinates: offsets from the shell's scene origin, so f32 is enough.
import * as THREE from 'three/webgpu';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { DecodedMesh, Vec3 } from '../bridge';
import { PointMark } from './markers';
import { valueColours } from './ramp';

interface GpuAdapterLike {
  limits: Record<string, number>;
  info?: { vendor?: string; architecture?: string; device?: string; description?: string };
}

interface GpuLike {
  requestAdapter(options?: { powerPreference?: string }): Promise<GpuAdapterLike | null>;
}

interface DeviceBackend {
  isWebGPUBackend?: boolean;
  device?: { queue: { onSubmittedWorkDone(): Promise<void> } };
}

export interface FrameStats {
  fps: number;
  worstFrameMs: number;
  triangles: number;
}

/** `performance.now()` when a frame's render was submitted, and when the GPU reported it done. */
export interface FrameTiming {
  submitted: number;
  gpuDone: number;
}

const SURFACE_COLOURS = [0xa8916b, 0x5b6b7d, 0x8c9aa6, 0xc9a227, 0x6f8f5f];

const PIVOT_MARK_PX = 9;
const PIVOT_MARK_COLOUR = 0xffd23f;

export class Viewport {
  readonly scene = new THREE.Scene();
  /** Loaded surfaces and their scale-test copies. */
  readonly content = new THREE.Group();
  /** Editing overlays (polylines, handles), drawn on top. */
  readonly overlay = new THREE.Group();
  /** Hooks run at the start of every frame with `performance.now()`. */
  readonly beforeRender = new Set<(now: number) => void>();
  onStats?: (stats: FrameStats) => void;
  /** What is drawn under a screen point, scene-local: what the view turns on and zooms to. */
  pickPoint?: (clientX: number, clientY: number) => Promise<Vec3 | null>;
  /** The rotation point, shown while a drag turns the view. */
  readonly pivotMark: PointMark;

  private surfaces: THREE.Group[] = [];
  private copies: THREE.Object3D[] = [];
  private frameWaiters: Array<(timing: FrameTiming) => void> = [];
  private statFrames = 0;
  private statStart = performance.now();
  private lastFrame = performance.now();
  private worstFrame = 0;
  private sceneRadius = 1000;
  private raycaster = new THREE.Raycaster();
  private resizeObserver: ResizeObserver;

  /** Plan view: looking straight down, north up, no perspective. */
  readonly orthographic = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 100_000);
  private plan = false;
  /** The section view: flat too, square onto a vertical plane with RL up. */
  readonly sectionCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 100_000);
  private sectioned = false;
  /** The view's centre before a section, to come back to. */
  private beforeSection = new THREE.Vector3();
  /** The 3D view's direction from its target, kept while in plan view. */
  private savedDirection = new THREE.Vector3(0.45, -0.75, 0.6).normalize();
  /** A left drag turning the 3D view: the point it turns on, and where the pointer last was. */
  private turning: { pivot: THREE.Vector3; x: number; y: number } | null = null;
  /** A wheel pick in flight: ticks that arrive meanwhile keep the depth it finds. */
  private settling = false;

  private constructor(
    readonly renderer: THREE.WebGPURenderer,
    readonly perspective: THREE.PerspectiveCamera,
    readonly controls: OrbitControls,
    readonly backend: string,
    readonly adapter: string,
    private host: HTMLElement,
  ) {
    this.orthographic.up.set(0, 1, 0);
    this.sectionCamera.up.set(0, 0, 1);
    this.scene.background = new THREE.Color(0x161e29);
    this.scene.add(new THREE.HemisphereLight(0xdfe8f2, 0x3a3a30, 1.4));
    const sun = new THREE.DirectionalLight(0xffffff, 1.8);
    sun.position.set(0.5, -0.8, 1.0);
    this.scene.add(sun, this.content, this.overlay);
    this.pivotMark = new PointMark(this, PIVOT_MARK_PX);
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(host);
    // The wheel zooms where the cursor is, and left drag is `turnAbout`, so the view works on what
    // is under the cursor, not on a centre left at the middle of the pit.
    controls.zoomToCursor = true;
    controls.mouseButtons.LEFT = null;
    const canvas = renderer.domElement;
    canvas.addEventListener('pointerdown', this.startTurn);
    canvas.addEventListener('pointermove', this.moveTurn);
    canvas.addEventListener('pointerup', this.endTurn);
    canvas.addEventListener('pointercancel', this.endTurn);
    canvas.addEventListener('wheel', this.settleDepth, { capture: true, passive: true });
    renderer.setAnimationLoop(this.frame);
  }

  static async create(host: HTMLElement): Promise<Viewport> {
    // Ask for the adapter's real buffer limits: the defaults (256 MB) are below one
    // multi-million-triangle index buffer.
    const gpu = (navigator as Navigator & { gpu?: GpuLike }).gpu;
    const adapter = gpu ? await gpu.requestAdapter({ powerPreference: 'high-performance' }) : null;
    const requiredLimits = adapter
      ? {
          maxBufferSize: adapter.limits.maxBufferSize,
          maxStorageBufferBindingSize: adapter.limits.maxStorageBufferBindingSize,
        }
      : undefined;
    const options = { antialias: true, powerPreference: 'high-performance', requiredLimits, reversedDepthBuffer: true };
    const renderer = new THREE.WebGPURenderer(options as ConstructorParameters<typeof THREE.WebGPURenderer>[0]);
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setSize(host.clientWidth, host.clientHeight);
    host.appendChild(renderer.domElement);
    await renderer.init();

    const camera = new THREE.PerspectiveCamera(45, host.clientWidth / host.clientHeight, 1, 100_000);
    camera.up.set(0, 0, 1); // mine grid: z is up
    camera.position.set(0, -2000, 1500);
    const controls = new OrbitControls(camera, renderer.domElement);

    const backend = (renderer.backend as DeviceBackend).isWebGPUBackend ? 'WebGPU' : 'WebGL2';
    const info = adapter?.info;
    const adapterName = info
      ? info.description || [info.vendor, info.architecture, info.device].filter(Boolean).join(' ') || 'unknown'
      : 'no WebGPU adapter';
    return new Viewport(renderer, camera, controls, backend, adapterName, host);
  }

  /** The camera in use: the 3D one, the plan one or the section one. */
  get camera(): THREE.PerspectiveCamera | THREE.OrthographicCamera {
    return this.sectioned ? this.sectionCamera : this.plan ? this.orthographic : this.perspective;
  }

  get planView(): boolean {
    return this.plan;
  }

  /** A view with no perspective: plan or section. A pixel measures the same everywhere in it. */
  get flatView(): boolean {
    return this.plan || this.sectioned;
  }

  get sectionView(): boolean {
    return this.sectioned;
  }

  /**
   * Looks square onto the vertical plane through scene-local plan points `a` and `b`, with `a` to the
   * left, framing the distances along it and the RLs. The solid view is hidden: only what is cut shows.
   */
  showSection(a: [number, number], b: [number, number], along: [number, number], rl: [number, number]): void {
    if (!this.sectioned) this.beforeSection.copy(this.controls.target);
    const length = Math.hypot(b[0] - a[0], b[1] - a[1]);
    const [dx, dy] = [(b[0] - a[0]) / length, (b[1] - a[1]) / length];
    const middle = (along[0] + along[1]) / 2;
    const centre = new THREE.Vector3(a[0] + dx * middle, a[1] + dy * middle, (rl[0] + rl[1]) / 2);
    const { clientWidth: w, clientHeight: h } = this.host;
    const aspect = w > 0 && h > 0 ? w / h : 1;
    const half = Math.max((rl[1] - rl[0]) / 2, (along[1] - along[0]) / 2 / aspect, 1) * 1.1;
    const camera = this.sectionCamera;
    Object.assign(camera, { left: -half * aspect, right: half * aspect, top: half, bottom: -half, zoom: 1 });
    camera.updateProjectionMatrix();
    // Standing to the right of the line from `a` to `b`, so it runs left to right on screen.
    const stand = Math.max(this.sceneRadius * 4, 1000);
    camera.position.set(centre.x + dy * stand, centre.y - dx * stand, centre.z);
    camera.lookAt(centre);
    this.sectioned = true;
    this.content.visible = false;
    this.controls.object = camera;
    this.controls.target.copy(centre);
    this.controls.enableRotate = false;
    this.controls.mouseButtons.LEFT = THREE.MOUSE.PAN;
    this.controls.update();
  }

  /** Slides the section's camera with its plane, so a step keeps the same view of the section. */
  moveSection(x: number, y: number): void {
    this.sectionCamera.position.x += x;
    this.sectionCamera.position.y += y;
    this.controls.target.x += x;
    this.controls.target.y += y;
  }

  /** Back to the view the section replaced. */
  closeSection(): void {
    this.sectioned = false;
    this.content.visible = true;
    this.controls.object = this.camera;
    this.controls.target.copy(this.beforeSection);
    this.controls.enableRotate = !this.plan;
    this.controls.mouseButtons.LEFT = this.plan ? THREE.MOUSE.PAN : null;
    this.controls.update();
  }

  /**
   * Switches between 3D and plan view over the same spot at about the same scale. Plan view pans
   * with the left button and zooms; it does not rotate.
   */
  setPlanView(on: boolean): void {
    if (on === this.plan) return;
    const target = this.controls.target.clone();
    const tan = Math.tan(THREE.MathUtils.degToRad(this.perspective.fov / 2));
    if (on) {
      const offset = this.perspective.position.clone().sub(target);
      this.savedDirection.copy(offset).normalize();
      this.setPlanHalfHeight(offset.length() * tan);
      this.orthographic.zoom = 1;
      this.placePlanCamera(target);
      this.controls.object = this.orthographic;
      this.controls.enableRotate = false;
      this.controls.mouseButtons.LEFT = THREE.MOUSE.PAN;
    } else {
      const half = (this.orthographic.top - this.orthographic.bottom) / 2 / this.orthographic.zoom;
      this.perspective.position.copy(target).addScaledVector(this.savedDirection, half / tan);
      this.controls.object = this.perspective;
      this.controls.enableRotate = true;
      this.controls.mouseButtons.LEFT = null;
    }
    this.plan = on;
    this.controls.update();
  }

  /** World units per screen pixel at `p`: keeps handles and markers one size on screen. */
  pixelSize(p: THREE.Vector3): number {
    const h = this.renderer.domElement.clientHeight || 1;
    if (this.flatView) {
      const flat = this.camera as THREE.OrthographicCamera;
      return (flat.top - flat.bottom) / flat.zoom / h;
    }
    return (this.perspective.position.distanceTo(p) * 2 * Math.tan(THREE.MathUtils.degToRad(this.perspective.fov / 2))) / h;
  }

  private setPlanHalfHeight(half: number): void {
    const { clientWidth: w, clientHeight: h } = this.host;
    const aspect = w > 0 && h > 0 ? w / h : 1;
    Object.assign(this.orthographic, { left: -half * aspect, right: half * aspect, top: half, bottom: -half });
    this.orthographic.updateProjectionMatrix();
  }

  /** The plan camera straight above `target`, high enough to see everything below it. */
  private placePlanCamera(target: THREE.Vector3): void {
    this.orthographic.position.set(target.x, target.y, target.z + Math.max(this.sceneRadius * 4, 1000));
    this.orthographic.lookAt(target);
  }

  /** Triangles in the visible surfaces and copies. */
  get triangleCount(): number {
    let total = 0;
    for (const m of [...this.surfaces, ...this.copies]) if (m.visible) total += m.userData.triangles as number;
    return total;
  }

  get baseTriangleCount(): number {
    return this.surfaces.reduce((sum, m) => sum + (m.userData.triangles as number), 0);
  }

  /**
   * Adds a surface as one mesh per chunk, all sharing one material. Normals are left out: with no
   * normal attribute three shades flat from screen-space derivatives. `release` runs when the
   * surface is removed (frees a shared buffer the chunks still view).
   */
  addMesh(decoded: DecodedMesh, release?: () => void, colour = SURFACE_COLOURS[this.surfaces.length % SURFACE_COLOURS.length]): THREE.Group {
    // A grid that carries values (seam thickness) is coloured by them, still shaded by the light.
    const value = decoded.header.value;
    const material = new THREE.MeshStandardMaterial({
      color: value ? 0xffffff : colour,
      vertexColors: value !== undefined,
      roughness: 0.92,
      metalness: 0,
      side: THREE.DoubleSide,
      flatShading: true,
    });
    const group = new THREE.Group();
    group.name = decoded.header.name;
    group.userData.triangles = decoded.header.triangles;
    group.userData.release = release;
    // Grid tiles of one size share an index array; share the attribute so it uploads once.
    const indexAttributes = new Map<Uint16Array, THREE.BufferAttribute>();
    for (const chunk of decoded.chunks) {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute('position', new THREE.BufferAttribute(chunk.positions, 3));
      if (value && chunk.values) geometry.setAttribute('color', new THREE.BufferAttribute(valueColours(chunk.values, value), 3, true));
      let index = indexAttributes.get(chunk.indices);
      if (!index) {
        index = new THREE.BufferAttribute(chunk.indices, 1);
        // Without this, three's WebGPU backend widens u16 indices to u32 on upload.
        index.normalized = true;
        indexAttributes.set(chunk.indices, index);
      }
      geometry.setIndex(index);
      // Bounds come from the shell, so three never walks the positions to find them.
      geometry.boundingBox = new THREE.Box3(new THREE.Vector3(...chunk.min), new THREE.Vector3(...chunk.max));
      geometry.boundingSphere = geometry.boundingBox.getBoundingSphere(new THREE.Sphere());
      group.add(new THREE.Mesh(geometry, material));
    }
    this.surfaces.push(group);
    this.content.add(group);
    return group;
  }

  removeMesh(group: THREE.Group): void {
    this.setCopies(1);
    this.surfaces = this.surfaces.filter((s) => s !== group);
    this.content.remove(group);
    let material: THREE.Material | undefined;
    for (const child of group.children) {
      const mesh = child as THREE.Mesh;
      mesh.geometry.dispose();
      material = mesh.material as THREE.Material;
    }
    material?.dispose();
    (group.userData.release as (() => void) | undefined)?.();
  }

  /** Tiles `n - 1` copies of the loaded surfaces beside them, sharing GPU buffers. Returns the scene's triangle count. */
  setCopies(n: number): number {
    for (const c of this.copies) this.content.remove(c);
    this.copies = [];
    if (n > 1 && this.surfaces.length > 0) {
      const box = new THREE.Box3();
      for (const s of this.surfaces) box.expandByObject(s);
      const size = box.getSize(new THREE.Vector3());
      const columns = Math.ceil(Math.sqrt(n));
      for (let k = 1; k < n; k++) {
        const offset = new THREE.Vector3((k % columns) * size.x * 1.05, Math.floor(k / columns) * size.y * 1.05, 0);
        for (const s of this.surfaces) {
          // Cloned chunk meshes share the original geometry and material.
          const copy = new THREE.Group();
          for (const child of s.children) copy.add((child as THREE.Mesh).clone());
          copy.position.copy(offset);
          copy.userData.triangles = s.userData.triangles;
          this.copies.push(copy);
          this.content.add(copy);
        }
      }
    }
    return this.triangleCount;
  }

  /** Frames everything loaded from the south-east, looking down at about 35 degrees. */
  fitView(): void {
    const box = new THREE.Box3().setFromObject(this.content);
    if (box.isEmpty()) return;
    const centre = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3());
    this.sceneRadius = size.length() / 2;
    this.controls.target.copy(centre);
    if (this.plan) {
      const { clientWidth: w, clientHeight: h } = this.host;
      this.setPlanHalfHeight(Math.max(size.y / 2, size.x / 2 / (w > 0 && h > 0 ? w / h : 1)) * 1.1);
      this.orthographic.zoom = 1;
      this.placePlanCamera(centre);
    } else {
      const direction = new THREE.Vector3(0.45, -0.75, 0.6).normalize();
      this.perspective.position.copy(centre).addScaledVector(direction, this.sceneRadius * 1.9);
    }
    this.controls.update();
  }

  /** Moves the view to look at `point` from the current direction, no farther than `distance`. */
  focusOn(point: Vec3, distance = 600): void {
    const target = new THREE.Vector3(...point);
    const offset = this.perspective.position.clone().sub(this.controls.target);
    this.controls.target.copy(target);
    if (this.plan) {
      this.placePlanCamera(target);
    } else {
      offset.setLength(Math.min(offset.length(), distance));
      this.perspective.position.copy(target).add(offset);
    }
    this.controls.update();
  }

  get bounds(): THREE.Box3 {
    return new THREE.Box3().setFromObject(this.content);
  }

  /** Resolves once the next frame has rendered and the GPU has finished it. */
  nextFrame(): Promise<FrameTiming> {
    return new Promise((resolve) => this.frameWaiters.push(resolve));
  }

  /** Shows only the surfaces whose names pass `keep` (all of them when `keep` is omitted). */
  showOnly(keep?: (name: string) => boolean): void {
    for (const s of this.surfaces) s.visible = keep ? keep(s.name) : true;
  }

  rayFromNdc(x: number, y: number): { origin: Vec3; direction: Vec3 } {
    this.raycaster.setFromCamera(new THREE.Vector2(x, y), this.camera);
    const { origin, direction } = this.raycaster.ray;
    return { origin: [origin.x, origin.y, origin.z], direction: [direction.x, direction.y, direction.z] };
  }

  clientToNdc(clientX: number, clientY: number): [number, number] {
    const rect = this.renderer.domElement.getBoundingClientRect();
    return [((clientX - rect.left) / rect.width) * 2 - 1, -((clientY - rect.top) / rect.height) * 2 + 1];
  }

  projectToClient(p: THREE.Vector3): [number, number] {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ndc = p.clone().project(this.camera);
    return [rect.left + ((ndc.x + 1) / 2) * rect.width, rect.top + ((1 - ndc.y) / 2) * rect.height];
  }

  dispose(): void {
    this.renderer.setAnimationLoop(null);
    this.resizeObserver.disconnect();
    this.controls.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private startTurn = (e: PointerEvent): void => {
    if (e.button !== 0 || this.flatView || !this.controls.enabled) return;
    const turn = { pivot: this.controls.target.clone(), x: e.clientX, y: e.clientY };
    this.turning = turn;
    // A drag over empty space keeps the view's centre. The answer takes about a millisecond.
    void this.pickPoint?.(e.clientX, e.clientY).then((hit) => {
      if (hit) turn.pivot.set(...hit);
    });
  };

  private moveTurn = (e: PointerEvent): void => {
    const turn = this.turning;
    if (!turn || !this.controls.enabled) return;
    const fullTurn = (2 * Math.PI) / (this.renderer.domElement.clientHeight || 1);
    this.turnAbout(turn.pivot, -fullTurn * (e.clientX - turn.x), fullTurn * (e.clientY - turn.y));
    this.pivotMark.show(turn.pivot, PIVOT_MARK_COLOUR);
    turn.x = e.clientX;
    turn.y = e.clientY;
  };

  private endTurn = (): void => {
    this.turning = null;
    this.pivotMark.hide();
  };

  /**
   * Swings the camera and its target together about `pivot`: `yaw` about the vertical, `raise` up
   * over the top (radians), stopping short of straight up or down. Moving both is what keeps the
   * pivot where it is on screen; the camera does not jump to look at it.
   */
  private turnAbout(pivot: THREE.Vector3, yaw: number, raise: number): void {
    const camera = this.perspective;
    const forward = camera.getWorldDirection(new THREE.Vector3());
    const right = forward.clone().cross(camera.up).normalize();
    const fromAbove = forward.angleTo(camera.up.clone().negate());
    const limit = 0.01;
    const raised = Math.min(raise, fromAbove - limit);
    const lowered = Math.max(raised, fromAbove - (Math.PI - limit));
    const swing = new THREE.Quaternion().setFromAxisAngle(camera.up, yaw).multiply(new THREE.Quaternion().setFromAxisAngle(right, -lowered));
    for (const point of [camera.position, this.controls.target]) {
      point.sub(pivot).applyQuaternion(swing).add(pivot);
    }
  }

  /**
   * Puts the view's target at the depth of what is under the cursor, along the line of sight so the
   * view does not move: zooming then slows as it nears the surface, and stops short of it.
   */
  private settleDepth = (e: WheelEvent): void => {
    if (this.flatView || this.settling || !this.pickPoint) return;
    this.settling = true;
    void this.pickPoint(e.clientX, e.clientY).then((hit) => {
      this.settling = false;
      if (!hit) return;
      const camera = this.perspective;
      const forward = camera.getWorldDirection(new THREE.Vector3());
      const depth = new THREE.Vector3(...hit).sub(camera.position).dot(forward);
      if (depth > 0) this.controls.target.copy(camera.position).addScaledVector(forward, depth);
    });
  };

  private frame = (): void => {
    const now = performance.now();
    for (const hook of this.beforeRender) hook(now);
    this.controls.update();
    this.updateClipPlanes();
    this.renderer.render(this.scene, this.camera);
    const submitted = performance.now();
    this.tickStats(now);
    if (this.frameWaiters.length > 0) {
      const waiters = this.frameWaiters;
      this.frameWaiters = [];
      const device = (this.renderer.backend as DeviceBackend).device;
      const done = device ? device.queue.onSubmittedWorkDone() : Promise.resolve();
      void done.then(() => {
        const gpuDone = performance.now();
        waiters.forEach((w) => w({ submitted, gpuDone }));
      });
    }
  };

  /** Near and far planes follow the orbit distance; the reversed depth buffer keeps precision. */
  private updateClipPlanes(): void {
    const distance = this.camera.position.distanceTo(this.controls.target);
    if (this.flatView) {
      const flat = this.camera as THREE.OrthographicCamera;
      flat.near = 0.1;
      flat.far = distance + this.sceneRadius * 6;
      flat.updateProjectionMatrix();
      return;
    }
    this.perspective.near = Math.max(0.05, distance / 5000);
    this.perspective.far = distance + this.sceneRadius * 6;
    this.perspective.updateProjectionMatrix();
  }

  private tickStats(now: number): void {
    this.statFrames++;
    this.worstFrame = Math.max(this.worstFrame, now - this.lastFrame);
    this.lastFrame = now;
    const elapsed = now - this.statStart;
    if (elapsed >= 500) {
      this.onStats?.({ fps: (this.statFrames * 1000) / elapsed, worstFrameMs: this.worstFrame, triangles: this.triangleCount });
      this.statFrames = 0;
      this.statStart = now;
      this.worstFrame = 0;
    }
  }

  private resize(): void {
    const { clientWidth: w, clientHeight: h } = this.host;
    if (w === 0 || h === 0) return;
    this.renderer.setSize(w, h);
    this.perspective.aspect = w / h;
    this.perspective.updateProjectionMatrix();
    this.setPlanHalfHeight((this.orthographic.top - this.orthographic.bottom) / 2);
    const section = this.sectionCamera;
    const half = (section.top - section.bottom) / 2;
    Object.assign(section, { left: (-half * w) / h, right: (half * w) / h });
    section.updateProjectionMatrix();
  }
}
