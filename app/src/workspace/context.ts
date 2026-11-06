import { createContext, useContext } from 'react';
import type { Colouring, Contours, LogEntry, ProjectSummary, RunRecord, Vec3 } from '@/bridge';
import type { RlShown, SeamShown } from '@/viewport/sceneSync';

/** What every panel reads, and the actions they share. App owns the state. */
export interface Workspace {
  summary: ProjectSummary | null;
  log: LogEntry[];
  /** The selected object (any kind), highlighted wherever it is drawn. */
  selected: string | null;
  /** The layer new strings are drawn on; created by the first string drawn on it. */
  activeLayer: string;
  /** Holds the viewport canvas. The viewport panel mounts it, so the canvas outlives panel moves. */
  viewportHost: HTMLDivElement;
  viewportMounted: () => void;
  select: (id: string | null) => void;
  /** Objects picked out together, from the tree or by Ctrl-clicking in the viewport. */
  marked: string[];
  setMarked: (ids: string[]) => void;
  /** Adds an object to the marks or takes it off, starting from what's selected. */
  toggleMark: (id: string) => void;
  setActiveLayer: (name: string) => void;
  setLayerVisible: (layer: string, visible: boolean) => void;
  /** Shows or hides objects one by one, under their layer's tick. Display only: not undone. */
  setObjectsVisible: (objects: string[], visible: boolean) => void;
  /**
   * Colours objects by RL or their layer, sets their opacity (0 to 1), or both; null leaves that part.
   * Display only: saved with the project, not undone. It shows at once; `commit` false is a preview, as a
   * slider moves, that the engine doesn't keep until a call that commits.
   */
  setLooks: (objects: string[], colouring: Colouring | null, opacity: number | null, commit?: boolean) => void;
  /** A layer's opacity, which multiplies its objects'; for solids it's the only one. As above. */
  setLayerOpacity: (layer: string, opacity: number, commit?: boolean) => void;
  /** The bands of colour by RL. Display only. */
  setContours: (contours: Contours) => void;
  /** What's coloured by RL on show, with its bands, for the legend. */
  rlShown: RlShown;
  /** Runs an engine command; reports the outcome on the status line. False if it failed. */
  run: (name: string, params: Record<string, unknown>) => Promise<boolean>;
  /** The same, handing back the run's record, or why it failed, for a panel to show. */
  runRecord: (name: string, params: Record<string, unknown>) => Promise<{ record?: RunRecord; error?: string }>;
  /** Takes a refusal off the status line, once what it was about has changed. */
  clearRefusal: () => void;
  /** Each seam model the viewport shows, on the horizon it shows coloured by thickness. */
  seamViews: SeamShown[];
  /** A section is showing in place of the plan view. */
  sectionOpen: boolean;
  /** A layer's own line weight in the section (px), or null for the defaults. Display only. */
  lineWeight: (layer: string) => number | null;
  setLineWeight: (layer: string, px: number | null) => void;
  /** Shows another horizon of a seam model. Display only: not a command, not undone. */
  showSeamHorizon: (id: string, horizon: string) => void;
  /** Shows the horizon an issue is on and flies the view to its worst node. */
  focusIssue: (id: string, horizon: string, issue: number) => void;
  /** Marks crossing sites (world coordinates) in the viewport, replacing any marked; [] clears them. */
  markCrossings: (sites: Vec3[]) => void;
  /** Flies the view to a point in world coordinates. */
  flyTo: (point: Vec3) => void;
  /** Frames a line (world coordinates) from the current direction. */
  frameLine: (points: Vec3[]) => void;
  /** Strings a click takes first where strings lie on each other (after the selected one): the lines on a step design's rows. */
  preferLines: (ids: string[]) => void;
  /** The next click in the viewport, snapped, in world coordinates. Null if it's cancelled with Esc. */
  pickPoint: (prompt: string) => Promise<Vec3 | null>;
  /** Two clicks in the viewport, with a line following the cursor from the first. Null if it's cancelled. */
  pickLine: (first: string, second: string) => Promise<[Vec3, Vec3] | null>;
  /** Clicks in the viewport until Enter, with a line following the cursor. Null if it's cancelled. */
  pickString: (prompt: string) => Promise<Vec3[] | null>;
  /** In Draw string mode. */
  drawing: boolean;
  /** Adds a typed vertex while drawing (`E N [RL]` or `@distance<bearing [RL|grade%]`); an error to show, or null. */
  typeVertex: (text: string) => Promise<string | null>;
}

export const WorkspaceContext = createContext<Workspace | null>(null);

export function useWorkspace(): Workspace {
  const workspace = useContext(WorkspaceContext);
  if (!workspace) throw new Error('useWorkspace outside the workspace');
  return workspace;
}
