// A layer's solids are one mesh (decision 0060, workstream 3). Each vertex says which solid it's of, and this table
// beside the mesh, one texel a solid, says whether that solid is hidden and whether it's selected. The shader reads it
// per vertex: a hidden solid's triangles fold to a point and draw nothing, and the highlight draws the same geometry
// again with only the selected solids left standing. Hiding a solid or box-selecting a pit's worth of them changes a
// few bytes a solid here, never the mesh.
import * as THREE from 'three/webgpu';
import { attribute, float, int, ivec2, positionLocal, textureLoad } from 'three/tsl';

/** Texels a row: 2,048 solids a row, so 200,000 solids are 98 rows. A power of two, so the sums below are exact. */
const WIDTH = 2048;

/** The flags of the solid a vertex is of, read from the table in the vertex stage: 1 or 0 each. */
function flagsOf(table: THREE.DataTexture) {
  const solid = attribute('solid', 'float');
  const texel = textureLoad(table, ivec2(int(solid.mod(WIDTH)), int(solid.div(WIDTH).floor())));
  return { hidden: texel.r, selected: texel.g };
}

export class SolidFlags {
  /** Each solid's number in the mesh, by id. */
  readonly index: Map<string, number>;
  readonly texture: THREE.DataTexture;
  private readonly data: Uint8Array;
  /** 1 where the vertex's solid is hidden, else 0; and the same for selected. Vertex stage. */
  readonly hidden: ReturnType<typeof flagsOf>['hidden'];
  readonly selected: ReturnType<typeof flagsOf>['selected'];
  /** How many are selected, as last set. */
  selectedCount = 0;

  constructor(readonly ids: readonly string[]) {
    this.index = new Map(ids.map((id, k) => [id, k]));
    const rows = Math.max(1, Math.ceil(ids.length / WIDTH));
    this.data = new Uint8Array(WIDTH * rows * 4);
    this.texture = new THREE.DataTexture(this.data, WIDTH, rows, THREE.RGBAFormat, THREE.UnsignedByteType);
    this.texture.magFilter = THREE.NearestFilter;
    this.texture.minFilter = THREE.NearestFilter;
    this.texture.generateMipmaps = false;
    this.texture.needsUpdate = true;
    ({ hidden: this.hidden, selected: this.selected } = flagsOf(this.texture));
  }

  private changed = false;
  private counting = 0;

  /** Starts a pass of `put`s over the solids; `end` sends what changed to the GPU. */
  begin(): void {
    this.changed = false;
    this.counting = 0;
  }

  /** One solid's flags: hidden, and selected. */
  put(id: string, hidden: boolean, selected: boolean): void {
    const k = this.index.get(id);
    if (k === undefined) return;
    const [h, s] = [hidden ? 255 : 0, selected ? 255 : 0];
    if (s && !h) this.counting++;
    if (this.data[4 * k] !== h || this.data[4 * k + 1] !== s) {
      this.data[4 * k] = h;
      this.data[4 * k + 1] = s;
      this.changed = true;
    }
  }

  /** Ends a pass: an unchanged table isn't sent to the GPU again. Returns whether it changed. */
  end(): boolean {
    this.selectedCount = this.counting;
    if (this.changed) this.texture.needsUpdate = true;
    return this.changed;
  }

  /** The material draws only the solids not hidden: the rest fold to a point. */
  hideHidden(material: THREE.NodeMaterial): void {
    material.positionNode = positionLocal.mul(float(1).sub(this.hidden));
  }

  /** The material draws only the solids selected and not hidden. */
  onlySelected(material: THREE.NodeMaterial): void {
    material.positionNode = positionLocal.mul(this.selected.mul(float(1).sub(this.hidden)));
  }

  dispose(): void {
    this.texture.dispose();
  }
}

/** Where a solid's flags sit in the table, as the shader finds them: for the tests. */
export function texelOf(solid: number): [number, number] {
  return [solid % WIDTH, Math.floor(solid / WIDTH)];
}
