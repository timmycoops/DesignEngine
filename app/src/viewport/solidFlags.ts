// A layer's solids are one mesh (decision 0060, workstream 3). Each vertex says which solid it's of, and this table
// beside the mesh, one texel a solid, says whether that solid is hidden and whether it's selected. The shader reads it
// per vertex: a hidden solid's triangles fold to a point and draw nothing, and the highlight draws the same geometry
// again with only the selected solids left standing. Hiding a solid or box-selecting a pit's worth of them changes a
// few bytes a solid here, never the mesh.
import * as THREE from 'three/webgpu';
import { attribute, float, int, ivec2, mix, positionLocal, step, textureLoad, uniform } from 'three/tsl';
import { bandColour } from './rlColour';

/** Texels a row: 2,048 solids a row, so 200,000 solids are 98 rows. A power of two, so the sums below are exact. */
const WIDTH = 2048;

/** The flags of the solid a vertex is of, read from the table in the vertex stage: 1 or 0 each. */
function flagsOf(table: THREE.DataTexture) {
  // Rounded: read per pixel, the number comes interpolated, and 5 can come as 4.99999.
  const solid = attribute('solid', 'float').add(0.5).floor();
  const texel = textureLoad(table, ivec2(int(solid.mod(WIDTH)), int(solid.div(WIDTH).floor())));
  return { hidden: texel.r, selected: texel.g, band: texel.b };
}

export class SolidFlags {
  /** Each solid's number in the mesh, by id. */
  readonly index: Map<string, number>;
  readonly texture: THREE.DataTexture;
  private readonly data: Uint8Array;
  /** 1 where the vertex's solid is hidden, else 0; and the same for selected. Vertex stage. */
  readonly hidden: ReturnType<typeof flagsOf>['hidden'];
  readonly selected: ReturnType<typeof flagsOf>['selected'];
  /** The band of a solid's value, 1 to 255 for bands 0 to 254, as a share of 255; 0 for none. */
  readonly band: ReturnType<typeof flagsOf>['band'];
  /** How many bands the ramp spreads over, for the colour of a solid's band. */
  readonly bandCount = uniform(1);
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
    ({ hidden: this.hidden, selected: this.selected, band: this.band } = flagsOf(this.texture));
  }

  private changed = false;
  private counting = 0;

  /** Starts a pass of `put`s over the solids; `end` sends what changed to the GPU. */
  begin(): void {
    this.changed = false;
    this.counting = 0;
  }

  /** One solid's flags: hidden, selected, and the band of its value if it's coloured by one. */
  put(id: string, hidden: boolean, selected: boolean, band: number | null = null): void {
    const k = this.index.get(id);
    if (k === undefined) return;
    const [h, s, b] = [hidden ? 255 : 0, selected ? 255 : 0, band === null ? 0 : Math.min(band + 1, 255)];
    if (s && !h) this.counting++;
    if (this.data[4 * k] !== h || this.data[4 * k + 1] !== s || this.data[4 * k + 2] !== b) {
      this.data[4 * k] = h;
      this.data[4 * k + 1] = s;
      this.data[4 * k + 2] = b;
      this.changed = true;
    }
  }

  /** The material takes a solid's band colour where its table gives one, and its own colour where not. */
  colourByBand(material: THREE.NodeMaterial): void {
    const band = this.band.mul(255).sub(1);
    const has = step(float(0.5 / 255), this.band);
    // colourByRl has set the material's colour as a colour; it stays that where there's no band.
    material.colorNode = mix(material.colorNode as unknown as THREE.Node<'vec3'>, bandColour(band, this.bandCount), has);
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
