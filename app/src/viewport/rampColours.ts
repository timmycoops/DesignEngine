// A ramp's colour, the same in the Face design panel and the viewport.

const COLOURS = ['#1d9e75', '#d85a30', '#7f77dd', '#d4537e', '#ba7517', '#378add'];

/** The colour for a ramp's name: its letter's place in the alphabet, round the list. */
export function rampColour(name: string): string {
  const letter = name.trim().toUpperCase().charCodeAt(0) - 65;
  return COLOURS[((Number.isNaN(letter) ? 0 : letter) % COLOURS.length + COLOURS.length) % COLOURS.length];
}
