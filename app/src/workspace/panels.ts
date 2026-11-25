/**
 * The panels that can be closed and reopened, and the dock each opens in (decision 0076): the tree on the left, a tool
 * as a tab in the tool dock on the right, beside Properties, and a table as a tab in the results dock under the view.
 * Kept free of React, so the tool list and the tests can read it.
 */
export type PanelId = 'layers' | 'properties' | 'strings' | 'from_surfaces' | 'surfaces' | 'move' | 'builder' | 'cut' | 'reserves' | 'workflow' | 'runlog' | 'restable' | 'case' | 'params' | 'canvas' | 'face' | 'licence' | 'viewcontrols';

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
  { id: 'from_surfaces', title: 'Strings from surfaces', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'surfaces', title: 'Surfaces', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'move', title: 'Move and copy', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'builder', title: 'Builder', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'cut', title: 'Cut solids', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'reserves', title: 'Reserves', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'workflow', title: 'Workflow', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'licence', title: 'Licence', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'viewcontrols', title: 'View controls', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'runlog', title: 'Run log', dock: 'results', size: 200 },
  // Every table, the objects and each reserves table (Tables and scripts plan, decision 1). Kept the Reserves table's id,
  // so a saved layout finds it.
  { id: 'restable', title: 'Tables', dock: 'results', size: 400 },
  // A reserves table's case a step at a time (Reserves case plan, decision 8).
  { id: 'case', title: 'Reserves case', dock: 'results', size: 520 },
  { id: 'params', title: 'Parameters', dock: 'results', size: 400 },
  { id: 'canvas', title: 'Workflow canvas', dock: 'results', size: 400 },
  { id: 'face', title: 'Face design', dock: 'results', size: 400 },
];
