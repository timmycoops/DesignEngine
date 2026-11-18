/**
 * The panels that can be closed and reopened, and the dock each opens in (decision 0076): the tree on the left, a tool
 * as a tab in the tool dock on the right, beside Properties, and a table as a tab in the results dock under the view.
 * Kept free of React, so the tool list and the tests can read it.
 */
export type PanelId = 'layers' | 'properties' | 'strings' | 'builder' | 'cut' | 'reserves' | 'workflow' | 'runlog' | 'blocks' | 'restable' | 'notebook' | 'params' | 'canvas' | 'face';

export type Dock = 'left' | 'tools' | 'results';

/** The tool dock's width: one width, so the view is as wide with every tool open as with one. */
export const TOOL_DOCK_WIDTH = 400;

/**
 * A dock at a time, with the room each panel asks for there: a width on the left and in the tool dock, and in the
 * results dock the height a table needs, which the dock grows to when it opens.
 */
export const PANELS: readonly { id: PanelId; title: string; dock: Dock; size: number }[] = [
  { id: 'layers', title: 'Layers', dock: 'left', size: 280 },
  { id: 'properties', title: 'Properties', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'strings', title: 'Strings', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'builder', title: 'Builder', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'cut', title: 'Cut solids', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'reserves', title: 'Reserves', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'workflow', title: 'Workflow', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'runlog', title: 'Run log', dock: 'results', size: 200 },
  { id: 'blocks', title: 'Objects table', dock: 'results', size: 320 },
  { id: 'restable', title: 'Reserves table', dock: 'results', size: 400 },
  { id: 'notebook', title: 'Dataflow', dock: 'results', size: 480 },
  { id: 'params', title: 'Parameters', dock: 'results', size: 400 },
  { id: 'canvas', title: 'Workflow canvas', dock: 'results', size: 400 },
  { id: 'face', title: 'Face design', dock: 'results', size: 400 },
];
