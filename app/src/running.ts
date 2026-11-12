/**
 * The one run going, app-wide (decision 0060, workstream 2). Runs go one at a time: while one goes, the view answers
 * (camera, snap, pick, box select, section, Show, Properties) and whatever would change the project waits, its button
 * saying what it's waiting on. The status bar shows the run, how far it has got and Cancel.
 */

export interface Going {
  command: string;
  /** `performance.now()` when it started. */
  since: number;
}

type Listener = () => void;

let going: Going | null = null;
const listeners = new Set<Listener>();
const tell = () => listeners.forEach((l) => l());

export const running = {
  get: (): Going | null => going,

  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },

  /** Marks `command` as the run going; false if one already is. */
  start(command: string, now = 0): boolean {
    if (going) return false;
    going = { command, since: now };
    tell();
    return true;
  },

  end() {
    going = null;
    tell();
  },
};

/** What a run is doing, in words, for the status bar and the buttons waiting on it. */
const WORDS: Record<string, string> = {
  'solid.slice_by_faces': 'slicing into strips',
  'solid.cut_by_surface': 'cutting solids',
  'solid.between_surfaces': 'making the solid between surfaces',
  'solid.import_many': 'importing solids',
  'solid.export_many': 'exporting solids',
  'solid.repair': 'repairing meshes',
  'solid.interrogate': 'interrogating blocks',
  'solid.compare': 'comparing values',
  'spry.import': 'importing from Spry',
  'deswik.import': 'importing from Deswik',
  'face.project': 'running the face design',
  'face.project_step': 'projecting the step',
  'face.build_step': 'building the step',
  'face.rebuild': 'rebuilding',
  'face.join': 'joining faces',
  'seam.build_model': 'building the seam model',
  'seam.relink': 'relinking grids',
  'face.step': 'adding the step',
  'surface.import': 'importing a surface',
  'solid.import': 'importing a solid',
  'string.import': 'importing strings',
  'grid.import_vulcan_seam': 'importing seam grids',
  'export.dxf': 'exporting to DXF',
  'export.csv': 'exporting to CSV',
};

export function runWords(command: string): string {
  return WORDS[command] ?? command;
}

/** "Waiting on slicing into strips", or null when no run is going. */
export function waitingOn(g: Going | null = going): string | null {
  return g ? `Waiting on ${runWords(g.command)}` : null;
}

/** How far a run has got, as the status bar says it: "3 of 15 strips · 12 s". */
export function progressWords(p: { done: number; total: number; of: string; seconds: number; cancelled?: boolean }): string {
  const time = p.seconds < 60 ? `${Math.floor(p.seconds)} s` : `${Math.floor(p.seconds / 60)} min ${Math.floor(p.seconds % 60)} s`;
  const count = p.total > 0 ? `${p.done.toLocaleString('en-AU')} of ${p.total.toLocaleString('en-AU')} ${p.of} · ` : '';
  return `${p.cancelled ? 'cancelling · ' : ''}${count}${time}`;
}
