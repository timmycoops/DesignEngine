import { createContext, useContext } from 'react';
import type { LogEntry, ProjectSummary, Vec3 } from '@/bridge';
import type { SeamShown } from '@/viewport/sceneSync';

/** What every panel reads, and the actions they share. App owns the state. */
export interface Workspace {
  summary: ProjectSummary | null;
  log: LogEntry[];
  /** The selected object (any kind); strings are also highlighted in the viewport. */
  selected: string | null;
  /** The layer new strings are drawn on; created by the first string drawn on it. */
  activeLayer: string;
  /** Holds the viewport canvas. The viewport panel mounts it, so the canvas outlives panel moves. */
  viewportHost: HTMLDivElement;
  viewportMounted: () => void;
  select: (id: string | null) => void;
  setActiveLayer: (name: string) => void;
  setLayerVisible: (layer: string, visible: boolean) => void;
  /** Runs an engine command; reports the outcome on the status line. False if it failed. */
  run: (name: string, params: Record<string, unknown>) => Promise<boolean>;
  /** The seam horizon the viewport shows coloured by thickness, if any. */
  seamView: SeamShown | null;
  /** Shows another horizon of a seam model. Display only: not a command, not undone. */
  showSeamHorizon: (id: string, horizon: string) => void;
  /** Shows the horizon an issue is on and flies the view to its worst node. */
  focusIssue: (id: string, horizon: string, issue: number) => void;
  /** Marks crossing sites (world coordinates) in the viewport, replacing any marked; [] clears them. */
  markCrossings: (sites: Vec3[]) => void;
  /** Flies the view to a point in world coordinates. */
  flyTo: (point: Vec3) => void;
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
