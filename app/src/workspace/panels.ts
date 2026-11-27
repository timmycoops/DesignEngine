/**
 * The panels that can be closed and reopened, and the dock each opens in (decision 0076): the tree on the left, a tool
 * as a tab in the tool dock on the right, beside Properties, and a table as a tab in the results dock under the view.
 * Kept free of React, so the tool list and the tests can read it.
 */
export type PanelId = 'layers' | 'properties' | 'strings' | 'from_surfaces' | 'surfaces' | 'move' | 'builder' | 'cut' | 'reserves' | 'workflow' | 'runlog' | 'restable' | 'case' | 'params' | 'face' | 'licence' | 'viewcontrols';

/**
 * Where a panel opens (decisions 0076, 0108): the tree on the left; the one tool slot on the right, a tool at a time;
 * the inspector (Properties) under it; the tables under the view; or a document over the view, as a tab beside it.
 */
export type Dock = 'left' | 'tools' | 'inspector' | 'results' | 'document';

/**
 * The side of the app a panel is on (decision 0108): Design for the view's work, Tables for the table work, `both` for
 * one that moves to whichever side asks for it (the Tables panel: the Objects table on Design, reserves tables on Tables).
 */
export type PanelSide = 'design' | 'tables' | 'both';

/** The tool dock's width: one width, so the view is as wide with every tool open as with one. */
export const TOOL_DOCK_WIDTH = 400;

/**
 * A dock at a time, with the room each panel asks for there: a width on the left and in the tool dock, and in the
 * results dock the height a table needs, which the dock grows to when it opens.
 */
export const PANELS: readonly { id: PanelId; title: string; dock: Dock; size: number; side?: PanelSide }[] = [
  { id: 'layers', title: 'Layers', dock: 'left', size: 280 },
  { id: 'properties', title: 'Properties', dock: 'inspector', size: TOOL_DOCK_WIDTH },
  { id: 'strings', title: 'Strings', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'from_surfaces', title: 'Strings from surfaces', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'surfaces', title: 'Surfaces', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'move', title: 'Move and copy', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'builder', title: 'Builder', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'cut', title: 'Cut solids', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'reserves', title: 'Reserves', dock: 'tools', size: TOOL_DOCK_WIDTH },
  // The workflow and its canvas, one document over the view (decision 0108, decision 7).
  { id: 'workflow', title: 'Workflow', dock: 'document', size: 0 },
  { id: 'licence', title: 'Licence', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'viewcontrols', title: 'View controls', dock: 'tools', size: TOOL_DOCK_WIDTH },
  { id: 'runlog', title: 'Run log', dock: 'results', size: 200 },
  // Every table, the objects and each reserves table (Tables and scripts plan, decision 1). Kept the Reserves table's id,
  // so a saved layout finds it.
  { id: 'restable', title: 'Tables', dock: 'results', size: 400, side: 'both' },
  // A reserves table's case a step at a time (Reserves case plan, decision 8).
  { id: 'case', title: 'Reserves case', dock: 'results', size: 520, side: 'tables' },
  { id: 'params', title: 'Parameters', dock: 'results', size: 400, side: 'tables' },
  // A design editor, not a result: a document over the view (decision 0108).
  { id: 'face', title: 'Face design', dock: 'document', size: 0 },
];

/** The side a panel is on: Design unless the list says otherwise. */
export const panelSide = (id: string): PanelSide => PANELS.find((p) => p.id === id)?.side ?? 'design';
