// Issue markers: a red diamond at the worst node of each blocking issue on the seam horizon shown,
// drawn over the surfaces at a fixed size on screen so a single bad node can be found on a 5 km grid.
import * as THREE from 'three/webgpu';
import type { Vec3 } from '../bridge';
import { ISSUE } from './ramp';
import type { Viewport } from './Viewport';

const MAX_MARKERS = 256;

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
    const eye = this.viewport.camera.position;
    for (let i = 0; i < this.points.length; i++) {
      this.dummy.position.copy(this.points[i]);
      this.dummy.scale.setScalar(eye.distanceTo(this.points[i]) * 0.009);
      this.dummy.updateMatrix();
      this.mesh.setMatrixAt(i, this.dummy.matrix);
    }
    this.mesh.count = this.points.length;
    this.mesh.instanceMatrix.needsUpdate = true;
  };
}
