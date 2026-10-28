// Full-window three.js viewport on WebGPURenderer (falls back to WebGL2 by itself).
// The scene is in local coordinates: offsets from the shell's scene origin, so f32 is enough.
import * as THREE from 'three/webgpu';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { DecodedMesh, Vec3 } from '../bridge';

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

export class Viewport {
  readonly scene = new THREE.Scene();
  /** Loaded surfaces and their scale-test copies. */
  readonly content = new THREE.Group();
  /** Editing overlays (polylines, handles), drawn on top. */
  readonly overlay = new THREE.Group();
  /** Hooks run at the start of every frame with `performance.now()`. */
  readonly beforeRender = new Set<(now: number) => void>();
  onStats?: (stats: FrameStats) => void;

  private surfaces: THREE.Mesh[] = [];
  private copies: THREE.Mesh[] = [];
  private frameWaiters: Array<(timing: FrameTiming) => void> = [];
  private statFrames = 0;
  private statStart = performance.now();
  private lastFrame = performance.now();
  private worstFrame = 0;
  private sceneRadius = 1000;
  private raycaster = new THREE.Raycaster();
  private resizeObserver: ResizeObserver;

  private constructor(
    readonly renderer: THREE.WebGPURenderer,
    readonly camera: THREE.PerspectiveCamera,
    readonly controls: OrbitControls,
    readonly backend: string,
    readonly adapter: string,
    private host: HTMLElement,
  ) {
    this.scene.background = new THREE.Color(0x161e29);
    this.scene.add(new THREE.HemisphereLight(0xdfe8f2, 0x3a3a30, 1.4));
    const sun = new THREE.DirectionalLight(0xffffff, 1.8);
    sun.position.set(0.5, -0.8, 1.0);
    this.scene.add(sun, this.content, this.overlay);
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(host);
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

  /** Triangles in the visible surfaces and copies. */
  get triangleCount(): number {
    let total = 0;
    for (const m of [...this.surfaces, ...this.copies]) if (m.visible) total += m.userData.triangles as number;
    return total;
  }

  get baseTriangleCount(): number {
    return this.surfaces.reduce((sum, m) => sum + (m.userData.triangles as number), 0);
  }

  addMesh(decoded: DecodedMesh): THREE.Mesh {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(decoded.positions, 3));
    geometry.setAttribute('normal', new THREE.BufferAttribute(decoded.normals, 3));
    geometry.setIndex(new THREE.BufferAttribute(decoded.indices, 1));
    const bounds = decoded.header.bounds_local;
    if (bounds) {
      // Bounds come from the shell, so three never walks millions of positions to find them.
      geometry.boundingBox = new THREE.Box3(new THREE.Vector3(...bounds.min), new THREE.Vector3(...bounds.max));
      geometry.boundingSphere = geometry.boundingBox.getBoundingSphere(new THREE.Sphere());
    }
    const colour = SURFACE_COLOURS[this.surfaces.length % SURFACE_COLOURS.length];
    const material = new THREE.MeshStandardMaterial({ color: colour, roughness: 0.92, metalness: 0, side: THREE.DoubleSide });
    const mesh = new THREE.Mesh(geometry, material);
    mesh.name = decoded.header.name;
    mesh.userData.triangles = decoded.header.triangles;
    this.surfaces.push(mesh);
    this.content.add(mesh);
    return mesh;
  }

  removeMesh(mesh: THREE.Mesh): void {
    this.setCopies(1);
    this.surfaces = this.surfaces.filter((m) => m !== mesh);
    this.content.remove(mesh);
    mesh.geometry.dispose();
    (mesh.material as THREE.Material).dispose();
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
          const copy = new THREE.Mesh(s.geometry, s.material);
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
    this.sceneRadius = box.getSize(new THREE.Vector3()).length() / 2;
    const direction = new THREE.Vector3(0.45, -0.75, 0.6).normalize();
    this.camera.position.copy(centre).addScaledVector(direction, this.sceneRadius * 1.9);
    this.controls.target.copy(centre);
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
    this.camera.near = Math.max(0.05, distance / 5000);
    this.camera.far = distance + this.sceneRadius * 6;
    this.camera.updateProjectionMatrix();
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
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }
}
