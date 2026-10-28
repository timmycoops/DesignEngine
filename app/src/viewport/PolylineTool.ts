// A polyline draped on surfaces: a 1 px line plus octahedron handles sized to stay a constant
// size on screen. Points are in scene-local coordinates.
import * as THREE from 'three/webgpu';
import type { Vec3 } from '../bridge';
import type { Viewport } from './Viewport';

const MAX_VERTICES = 4096;
const HANDLE_SCREEN_SCALE = 0.006;

export class PolylineTool {
  readonly points: THREE.Vector3[] = [];
  private positions: THREE.BufferAttribute;
  private line: THREE.Line;
  private handles: THREE.InstancedMesh;
  private dummy = new THREE.Object3D();

  constructor(private viewport: Viewport) {
    const geometry = new THREE.BufferGeometry();
    this.positions = new THREE.BufferAttribute(new Float32Array(MAX_VERTICES * 3), 3);
    this.positions.setUsage(THREE.DynamicDrawUsage);
    geometry.setAttribute('position', this.positions);
    geometry.setDrawRange(0, 0);
    this.line = new THREE.Line(geometry, new THREE.LineBasicMaterial({ color: 0xffd23f, depthTest: false }));
    this.line.renderOrder = 10;
    this.line.frustumCulled = false;

    const handleMaterial = new THREE.MeshBasicMaterial({ color: 0xffffff, depthTest: false });
    this.handles = new THREE.InstancedMesh(new THREE.OctahedronGeometry(1), handleMaterial, MAX_VERTICES);
    this.handles.count = 0;
    this.handles.renderOrder = 11;
    this.handles.frustumCulled = false;

    viewport.overlay.add(this.line, this.handles);
    viewport.beforeRender.add(() => this.scaleHandles());
  }

  add(p: Vec3): void {
    if (this.points.length >= MAX_VERTICES) return;
    this.points.push(new THREE.Vector3(...p));
    this.sync();
  }

  move(index: number, p: Vec3): void {
    this.points[index].set(...p);
    this.positions.setXYZ(index, p[0], p[1], p[2]);
    this.positions.needsUpdate = true;
    this.scaleHandles();
  }

  clear(): void {
    this.points.length = 0;
    this.sync();
  }

  /** Index of the vertex within `radiusPx` of a client point, or -1. */
  pick(clientX: number, clientY: number, radiusPx = 12): number {
    let best = -1;
    let bestDist = radiusPx * radiusPx;
    this.points.forEach((p, i) => {
      const [x, y] = this.viewport.projectToClient(p);
      const d = (x - clientX) ** 2 + (y - clientY) ** 2;
      if (d <= bestDist) {
        best = i;
        bestDist = d;
      }
    });
    return best;
  }

  private sync(): void {
    this.points.forEach((p, i) => this.positions.setXYZ(i, p.x, p.y, p.z));
    this.positions.needsUpdate = true;
    this.line.geometry.setDrawRange(0, this.points.length);
    this.handles.count = this.points.length;
    this.scaleHandles();
  }

  private scaleHandles(): void {
    const eye = this.viewport.camera.position;
    this.points.forEach((p, i) => {
      this.dummy.position.copy(p);
      this.dummy.scale.setScalar(eye.distanceTo(p) * HANDLE_SCREEN_SCALE);
      this.dummy.updateMatrix();
      this.handles.setMatrixAt(i, this.dummy.matrix);
    });
    this.handles.instanceMatrix.needsUpdate = true;
  }
}
