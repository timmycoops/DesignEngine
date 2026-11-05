// Colour by RL: bands of colour between round RLs, with a thin contour line at each band edge, worked
// out in the fragment shader (TSL, so the same node graph runs on WebGPU and WebGL2). Nothing per vertex
// is built in JS, so a surface of millions of triangles changes colour by changing a few uniforms.
//
// The GPU has f32 positions less the scene origin. Band edges are at whole multiples of the interval in
// true RL, so the first edge is passed in local z: the origin's z is a whole metre (state.rs `origin`),
// so `base` below is exact and every edge lands on a round RL.
import * as THREE from 'three/webgpu';
import { clamp, float, floor, fract, fwidth, min, mix, positionLocal, smoothstep, texture, uniform, vec2 } from 'three/tsl';
import type { Bands } from './bands';
import { DEFAULT_RL_PALETTE, paletteColour, RL_PALETTES, type RlPaletteName } from './rlPalettes';

export { autoInterval, type Bands, bandsFor, INTERVALS } from './bands';

/** The ramp RLs are coloured on (`rlPalettes.ts`), one for every surface at once. */
let palette: RlPaletteName = DEFAULT_RL_PALETTE;

export const rlPalette = (): RlPaletteName => palette;

/** Colours every surface on another ramp: the one texture they all sample is rewritten, no shader is rebuilt. */
export function setRlPalette(name: RlPaletteName): void {
  if (name === palette || !(name in RL_PALETTES)) return;
  palette = name;
  if (rampTexture) {
    fillRamp(rampTexture.image.data as Uint8Array);
    rampTexture.needsUpdate = true;
  }
}

/** The ramp colour at `t` in [0, 1], as sRGB bytes. */
export function rlColour(t: number): [number, number, number] {
  return paletteColour(RL_PALETTES[palette].stops, t);
}

/** The ramp as a CSS gradient, low RL on the left. */
export function rlRampCss(): string {
  const stops = RL_PALETTES[palette].stops;
  const css = stops.map(([r, g, b], i) => `rgb(${r} ${g} ${b}) ${((100 * i) / (stops.length - 1)).toFixed(1)}%`);
  return `linear-gradient(to right, ${css.join(', ')})`;
}

const RAMP_WIDTH = 256;

function fillRamp(data: Uint8Array): void {
  for (let i = 0; i < RAMP_WIDTH; i++) {
    const [r, g, b] = rlColour(i / (RAMP_WIDTH - 1));
    data.set([Math.round(r), Math.round(g), Math.round(b), 255], 4 * i);
  }
}

/** The ramp as a texture the shader samples, made once and shared by every surface. */
let rampTexture: THREE.DataTexture | null = null;
function ramp(): THREE.DataTexture {
  if (rampTexture) return rampTexture;
  const data = new Uint8Array(RAMP_WIDTH * 4);
  fillRamp(data);
  const t = new THREE.DataTexture(data, RAMP_WIDTH, 1, THREE.RGBAFormat);
  t.colorSpace = THREE.SRGBColorSpace;
  t.magFilter = THREE.LinearFilter;
  t.minFilter = THREE.LinearFilter;
  t.needsUpdate = true;
  rampTexture = t;
  return t;
}

/** The uniforms one surface's material is drawn with. */
export interface RlUniforms {
  layerColour: THREE.UniformNode<"color", THREE.Color>;
  /** 1 colours by RL, 0 by the layer. */
  byRl: THREE.UniformNode<"float", number>;
  base: THREE.UniformNode<"float", number>;
  interval: THREE.UniformNode<"float", number>;
  count: THREE.UniformNode<"float", number>;
  /** 1 draws contour lines. */
  lines: THREE.UniformNode<"float", number>;
}

/** How dark a contour line is against its band. */
const LINE_SHADE = 0.42;

/**
 * Sets a material's colour to the RL bands, or the layer's colour, by uniform: switching between the two
 * or moving the bands never rebuilds the shader.
 */
export function colourByRl(material: THREE.MeshStandardNodeMaterial, colour: number): RlUniforms {
  const u: RlUniforms = {
    layerColour: uniform(new THREE.Color(colour)),
    byRl: uniform(1),
    base: uniform(0),
    interval: uniform(10),
    count: uniform(1),
    lines: uniform(1),
  };
  // Bands from the lowest edge: whole numbers at the edges.
  const b = positionLocal.z.sub(u.base).div(u.interval);
  const band = clamp(floor(b), float(0), u.count.sub(1));
  const map = ramp();
  const stepped = texture(map, vec2(band.add(0.5).div(u.count), 0.5)).rgb;
  // Zoomed out until a band is under a few pixels, the bands blur into the smooth ramp, not a moire.
  const perPixel = fwidth(b);
  const smooth = texture(map, vec2(clamp(b.div(u.count), float(0), float(1)), 0.5)).rgb;
  const banded = mix(stepped, smooth, smoothstep(0.25, 0.6, perPixel));
  // A line about a pixel wide either side of each edge, faded out where edges crowd closer than a few pixels.
  const fromEdge = min(fract(b), float(1).sub(fract(b)));
  const onLine = float(1).sub(smoothstep(perPixel.mul(0.5), perPixel.mul(1.5), fromEdge));
  const crowded = float(1).sub(smoothstep(0.12, 0.3, perPixel));
  const lined = mix(banded, banded.mul(LINE_SHADE), onLine.mul(crowded).mul(u.lines));
  material.colorNode = mix(u.layerColour, lined, u.byRl);
  return u;
}

/** Puts one surface's bands into its uniforms. */
export function setBands(u: RlUniforms, bands: Bands, lines: boolean): void {
  u.base.value = bands.base;
  u.interval.value = bands.interval;
  u.count.value = bands.count;
  u.lines.value = lines ? 1 : 0;
}
