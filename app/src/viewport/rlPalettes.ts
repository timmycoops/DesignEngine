// Ramps an RL can be coloured on, low RL first, as sRGB bytes. None of them is viridis, the seam thickness
// ramp, so an RL is never read as a thickness. Plain data, so `pnpm test` can check them without three.
//
// Spectral is the one the app draws with, Tim's pick from all five on PIT B (0043). The others stay so the
// bench can draw them side by side again (`--pictures <folder> --palettes`); nothing in the app offers them.

export type Rgb = [number, number, number];

export interface RlPalette {
  label: string;
  /** Evenly spaced stops, low RL first. */
  stops: Rgb[];
}

export const RL_PALETTES = {
  /** Crameri's batlow: blue to green to ochre to pink, lightness rising evenly. */
  batlow: {
    label: 'Batlow',
    stops: [
      [16, 63, 96],
      [28, 90, 98],
      [60, 109, 86],
      [104, 123, 62],
      [157, 137, 43],
      [210, 147, 67],
      [248, 161, 123],
      [253, 183, 188],
      [250, 204, 250],
    ],
  },
  /**
   * Google's turbo (Mikhailov, 2019; Apache 2.0) from its polynomial, 0.06 to 0.94 of the way along so the
   * ends are blue and red, not near black: the blue to red rainbow Vulcan and Deswik users know, smoothed.
   */
  turbo: {
    label: 'Rainbow (turbo)',
    stops: [
      [73, 61, 171],
      [66, 112, 242],
      [45, 162, 243],
      [37, 205, 208],
      [54, 237, 160],
      [95, 252, 115],
      [150, 250, 80],
      [206, 231, 56],
      [248, 196, 42],
      [255, 149, 32],
      [247, 97, 24],
      [206, 49, 13],
      [160, 17, 1],
    ],
  },
  /** Hypsometric tints, as on a topographic map: greens low, then straw, tans and browns, then pale. */
  terrain: {
    label: 'Terrain',
    stops: [
      [38, 115, 77],
      [92, 150, 80],
      [158, 186, 98],
      [222, 214, 140],
      [214, 172, 104],
      [178, 124, 74],
      [140, 98, 76],
      [178, 160, 150],
      [245, 242, 238],
    ],
  },
  /** ColorBrewer's Spectral (Brewer, Apache 2.0), eleven classes, reversed: purple-blue to pale yellow to red. */
  spectral: {
    label: 'Spectral',
    stops: [
      [94, 79, 162],
      [50, 136, 189],
      [102, 194, 165],
      [171, 221, 164],
      [230, 245, 152],
      [255, 255, 191],
      [254, 224, 139],
      [253, 174, 97],
      [244, 109, 67],
      [213, 62, 79],
      [158, 1, 66],
    ],
  },
  /**
   * Blue to green to orange to red at one lightness and one chroma (OKLCH 0.72, 0.12, hue 260° down to 25° in
   * even steps), so every band is as far from the next and the light alone shows the shape.
   */
  even: {
    label: 'Even rainbow',
    stops: [
      [120, 165, 239],
      [79, 175, 230],
      [34, 184, 209],
      [30, 188, 180],
      [75, 188, 147],
      [119, 184, 113],
      [156, 175, 83],
      [188, 164, 65],
      [211, 151, 71],
      [225, 140, 95],
      [230, 133, 126],
    ],
  },
} satisfies Record<string, RlPalette>;

export type RlPaletteName = keyof typeof RL_PALETTES;

export const RL_PALETTE_NAMES = Object.keys(RL_PALETTES) as RlPaletteName[];

export const DEFAULT_RL_PALETTE: RlPaletteName = 'spectral';

/** The colour `t` of the way along a ramp, `t` in [0, 1], as sRGB bytes (not rounded). */
export function paletteColour(stops: readonly Rgb[], t: number): Rgb {
  const x = Math.min(Math.max(t, 0), 1) * (stops.length - 1);
  const i = Math.min(Math.floor(x), stops.length - 2);
  const f = x - i;
  const [a, b] = [stops[i], stops[i + 1]];
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}
