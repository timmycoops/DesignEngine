// Issue markers: a red diamond at the worst node of each blocking issue on the seam horizon shown,
// drawn over the surfaces at a fixed size on screen so a single bad node can be found on a 5 km grid.
import * as THREE from 'three/webgpu';
import type { Vec3 } from '../bridge';
import { ISSUE } from './ramp';
import type { Viewport } from './Viewport';

const MAX_MARKERS = 256;
/** A point mark's ring and centre dot, as shares of its radius. */
const RING_INSIDE = 0.72;
const DOT = 0.2;

/** A ring with a dot at one point, drawn over everything, facing the camera, one size on screen. */
export class PointMark {
  private mark = new THREE.Group();
  private material = new THREE.MeshBasicMaterial({ depthTest: false, side: THREE.DoubleSide });

  constructor(
    private viewport: Viewport,
    private radiusPx: number,
  ) {
    for (const geometry of [new THREE.RingGeometry(RING_INSIDE, 1, 40), new THREE.CircleGeometry(DOT, 16)]) {
      const mesh = new THREE.Mesh(geometry, this.material);
      mesh.renderOrder = 14;
      mesh.frustumCulled = false;
      this.mark.add(mesh);
    }
    this.mark.visible = false;
    viewport.overlay.add(this.mark);
    viewport.beforeRender.add(this.place);
  }

  /** Where the mark is (scene-local), or null when it is hidden. */
  get position(): THREE.Vector3 | null {
    return this.mark.visible ? this.mark.position : null;
  }

  show(at: THREE.Vector3, colour: number): void {
    this.mark.position.copy(at);
    this.material.color.setHex(colour);
    this.mark.visible = true;
  }

  hide(): void {
    this.mark.visible = false;
  }

  private place = (): void => {
    if (!this.mark.visible) return;
    this.mark.quaternion.copy(this.viewport.camera.quaternion);
    this.mark.scale.setScalar(this.viewport.pixelSize(this.mark.position) * this.radiusPx);
  };
}

export class IssueMarkers {
  private mesh: THREE.InstancedMesh;
  private points: THREE.Vector3[] = [];
  private dummy = new THREE.Object3D();

  constructor(private viewport: Viewport) {
    const [r, g, b] = ISSUE;
    const material = new THREE.MeshBasicMaterial({ color: new THREE.Color(r / 255, g / 255, b / 255), depthTest: false });
    this.mesh = new THREE.InstancedMesh(new THREE.OctahedronGeometry(1), material, MAX_MARKERS);
    this.mesh.count = 0;
    this.mesh.renderOrder = 12;
    this.mesh.frustumCulled = false;
    viewport.overlay.add(this.mesh);
    viewport.beforeRender.add(this.place);
  }

  /** Scene-local positions; replaces the markers shown. */
  set(positions: Vec3[]): void {
    this.points = positions.slice(0, MAX_MARKERS).map((p) => new THREE.Vector3(...p));
  }

  private place = (): void => {
    for (let i = 0; i < this.points.length; i++) {
      this.dummy.position.copy(this.points[i]);
      this.dummy.scale.setScalar(this.viewport.pixelSize(this.points[i]) * 9);
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);
    }
    this.mesh.count = this.points.length;
    this.mesh.instanceMatrix.needsUpdate = true;
  };
}
