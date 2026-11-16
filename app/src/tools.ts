/**
 * The one list of tools (decision 0076, workstream 4): everything the app can be asked to do from its toolbar, its
 * menus, the Panels menu and the workflow canvas, each tool once, in the stage of the work it belongs to. The toolbar,
 * the menus and the canvas are drawn from it, and Find a command will search it, so a new tool is added here and
 * nowhere else. Kept free of React, so the tests can read it: an icon is a name (`toolIcons.tsx` holds the pictures) and
 * what a tool does is a panel to open, or an action App carries out. Nothing is imported at run time, for the same reason.
 */
import type { PanelId } from './workspace/panels';

/** The stages of the work, in the order it's done, and `always` for what's there whatever the stage. */
export type Stage = 'project' | 'strings' | 'walls' | 'solids' | 'geology' | 'reserves' | 'always';

export const STAGES: readonly { id: Exclude<Stage, 'always'>; title: string }[] = [
  { id: 'project', title: 'Project' },
  { id: 'strings', title: 'Strings' },
  { id: 'walls', title: 'Walls' },
  { id: 'solids', title: 'Solids' },
  { id: 'geology', title: 'Geology' },
  { id: 'reserves', title: 'Reserves' },
];

/** The pictures tools are drawn with, by name. */
export type ToolIcon =
  | 'new'
  | 'open'
  | 'save'
  | 'save-as'
  | 'undo'
  | 'redo'
  | 'import'
  | 'export'
  | 'face'
  | 'builder'
  | 'cut'
  | 'reserves'
  | 'blocks'
  | 'reserves-table'
  | 'parameters'
  | 'workflow'
  | 'panels';

/** What App carries out itself: files, undo, the seam model, the layout. */
export type ToolAction =
  | 'project.new'
  | 'project.open'
  | 'project.open_folder'
  | 'project.save'
  | 'project.save_as'
  | 'edit.undo'
  | 'edit.redo'
  | 'import.surfaces'
  | 'import.vulcan_grid'
  | 'import.strings'
  | 'import.solids'
  | 'import.solids_folder'
  | 'import.solids_repaired'
  | 'import.solids_folder_repaired'
  | 'import.deswik'
  | 'import.spry'
  | 'seam.suggest'
  | 'seam.build'
  | 'seam.relink'
  | 'export.selected_dxf'
  | 'export.selected_strings'
  | 'export.selected_solids'
  | 'export.visible_dxf'
  | 'export.visible_strings'
  | 'export.visible_solids'
  | 'solids.repair'
  | 'panels.hide'
  | 'panels.reset';

/** What a tool needs before it can be used: something to undo, or something of a kind selected. It's off without it. */
export type ToolNeeds = 'undo' | 'redo' | 'selected-dxf' | 'selected-strings' | 'selected-solids';

export type MenuId = 'project' | 'import' | 'export';

/** Where a tool sits in one of the toolbar's menus. */
export interface MenuPlace {
  in: MenuId;
  /** Its place down the menu. */
  at: number;
  /** As the menu words it, where that's not its label ("Surfaces…" under Import). */
  name?: string;
  /** A menu inside the menu that holds it, with what the tip on its name says. */
  under?: { name: string; tip: string };
  /** A rule above it. */
  rule?: boolean;
  /** A heading above it. */
  heading?: string;
}

export interface Tool {
  /** Unique, and stable: a drag onto the canvas carries it. */
  key: string;
  stage: Stage;
  /** Its name in words, as a row of tools or Find a command shows it. */
  label: string;
  /** What it does, in a line. For a panel's choice this is the line at the panel's head. */
  tip: string;
  icon?: ToolIcon;
  /** The keys that do the same, as written for people ("Ctrl+S"). */
  shortcut?: string;
  /** The panel it opens. A tool whose key is its panel's id opens the panel as it stands. */
  panel?: PanelId;
  /** Which of what the panel does it sets the panel to, as the panel's choice holds it. */
  mode?: string;
  /** As the panel's choice words it, where that's not its label. */
  choice?: string;
  /** The engine command it runs, through its panel or its action. */
  command?: string;
  /** What App does for it, where it opens no panel. */
  action?: ToolAction;
  needs?: ToolNeeds;
  menu?: MenuPlace;
  /** It can add its step to a workflow (decision 0065): what the step works on, in a few words. */
  step?: { takes: string };
  /** Its name on today's one row of the toolbar, until the row becomes a tab a stage (workstream 5). */
  short?: string;
}

const REPAIRED = 'Sheets and fins off, cracks joined, holes up to 10 % filled';
const SOLIDS_MENU = { name: 'Solids', tip: 'Closed triangulations in Vulcan .00t or DXF files, or a zip or folder of them' };

export const TOOLS: readonly Tool[] = [
  // Project: the file, and getting data in and out.
  { key: 'project.new', stage: 'project', label: 'New project', tip: 'Starts an empty project', icon: 'new', action: 'project.new', menu: { in: 'project', at: 1 } },
  { key: 'project.open', stage: 'project', label: 'Open a project', tip: 'Opens a .zdp project file', icon: 'open', action: 'project.open', menu: { in: 'project', at: 2, name: 'Open a project (.zdp)…' } },
  { key: 'project.open_folder', stage: 'project', label: 'Open an older project folder', tip: 'Opens a .zdproj folder, which saves as a .zdp beside it', icon: 'open', action: 'project.open_folder', menu: { in: 'project', at: 3, name: 'Open an older project folder (.zdproj)…' } },
  { key: 'project.save_as', stage: 'project', label: 'Save as', tip: 'Saves the project as another .zdp file', icon: 'save-as', action: 'project.save_as', menu: { in: 'project', at: 5, name: 'Save as…' } },
  { key: 'import.surfaces', stage: 'project', label: 'Import surfaces', tip: 'Vulcan .00t, DXF 3DFACE or polyface, or an XYZ or CSV grid', command: 'surface.import', action: 'import.surfaces', menu: { in: 'import', at: 1, name: 'Surfaces…' } },
  { key: 'import.strings', stage: 'project', label: 'Import strings', tip: 'DXF polylines and lines, or CSV with x, y and z columns', command: 'string.import', action: 'import.strings', menu: { in: 'import', at: 3, name: 'Strings…' } },
  { key: 'import.deswik', stage: 'project', label: 'Import a Deswik drawing', tip: 'Layers of a Deswik drawing: strings with their projection details, surfaces and solids', command: 'deswik.import', action: 'import.deswik', menu: { in: 'import', at: 8, name: 'Deswik drawing (.dcf)…', rule: true } },
  { key: 'import.spry', stage: 'project', label: 'Import a Spry project', tip: "Block solids named by their node paths with Spry's values, and design-layer strings and solids", command: 'spry.import', action: 'import.spry', menu: { in: 'import', at: 9, name: 'Spry project (.spry)…' } },
  { key: 'export.selected_dxf', stage: 'project', label: 'Export what is selected to DXF', tip: 'The selected strings, surfaces and solids as one DXF', action: 'export.selected_dxf', needs: 'selected-dxf', menu: { in: 'export', at: 1, name: 'Selected to DXF…' } },
  { key: 'export.selected_strings', stage: 'project', label: 'Export selected strings to CSV', tip: 'The selected strings as x, y and z rows', action: 'export.selected_strings', needs: 'selected-strings', menu: { in: 'export', at: 2, name: 'Selected strings to CSV…' } },
  { key: 'export.visible_dxf', stage: 'project', label: 'Export everything visible to DXF', tip: 'Every string, surface and solid on show as one DXF', action: 'export.visible_dxf', menu: { in: 'export', at: 4, name: 'Everything visible to DXF…' } },
  { key: 'export.visible_strings', stage: 'project', label: 'Export visible strings to CSV', tip: 'Every string on show as x, y and z rows', action: 'export.visible_strings', menu: { in: 'export', at: 5, name: 'Visible strings to CSV…' } },

  // Strings: what Properties runs on a picked string, until they come out of it (workstream 7).
  { key: 'string.offset', stage: 'strings', label: 'Offset', tip: 'Offsets a string to one side, once or several times', panel: 'properties', command: 'string.offset' },
  { key: 'string.number_across', stage: 'strings', label: 'Number across', tip: 'Numbers strips by a line drawn across the strings', panel: 'properties', command: 'string.number_across' },
  { key: 'string.polygons_from_lines', stage: 'strings', label: 'Polygons from lines', tip: 'Makes named block polygons from lines across the strips and lines closing them', panel: 'properties', command: 'string.polygons_from_lines' },
  { key: 'string.set_grade', stage: 'strings', label: 'Set grade', tip: 'Sets a string to a grade from one end', panel: 'properties', command: 'string.set_grade' },
  { key: 'string.set_elevation', stage: 'strings', label: 'Set elevation', tip: 'Sets a string to one RL', panel: 'properties', command: 'string.set_elevation' },
  { key: 'string.drape', stage: 'strings', label: 'Drape', tip: 'Drapes a string onto a grid or surface', panel: 'properties', command: 'string.drape' },
  { key: 'string.split', stage: 'strings', label: 'Split', tip: 'Splits a string at a vertex', panel: 'properties', command: 'string.split' },
  { key: 'string.join', stage: 'strings', label: 'Join', tip: 'Joins strings end to end', panel: 'properties', command: 'string.join' },
  { key: 'string.smooth', stage: 'strings', label: 'Smooth', tip: 'Smooths a string', panel: 'properties', command: 'string.smooth' },
  { key: 'string.filter', stage: 'strings', label: 'Filter', tip: 'Takes out vertices a string does not need', panel: 'properties', command: 'string.filter' },
  { key: 'string.reverse', stage: 'strings', label: 'Reverse', tip: 'Turns a string to run the other way', panel: 'properties', command: 'string.reverse' },
  { key: 'string.set_closed', stage: 'strings', label: 'Close', tip: 'Closes a string, or opens it', panel: 'properties', command: 'string.set_closed' },

  // Walls.
  { key: 'face', stage: 'walls', label: 'Face design', tip: 'lines, profiles and rules, run as one', icon: 'face', panel: 'face', short: 'Face' },
  { key: 'builder', stage: 'walls', label: 'Builder', tip: 'pick a line, build one wall or berm from it, edit between steps', icon: 'builder', panel: 'builder', short: 'Builder' },
  { key: 'builder:step', stage: 'walls', label: 'Build a step', tip: 'Builds one wall or berm from the line you pick.', panel: 'builder', command: 'face.build_step', step: { takes: 'the line picked' } },
  { key: 'builder:rebuild', stage: 'walls', label: 'Rebuild from here', tip: 'Builds the steps above an edited line again, each with its settings', panel: 'builder', command: 'face.rebuild' },
  { key: 'cut:join_faces', stage: 'walls', label: 'Join faces', tip: 'Makes one face a strip from up and down faces, several runs or a build’s steps', panel: 'cut', mode: 'strips', command: 'face.join' },

  // Solids. Cut solids' six, in the order the work is done; the panel's own choice keeps its order.
  { key: 'cut', stage: 'solids', label: 'Cut solids', tip: 'into strips by faces, by a surface, grid or RL, or one between two surfaces', icon: 'cut', panel: 'cut', short: 'Cut' },
  { key: 'cut:between', stage: 'solids', label: 'Between surfaces', tip: 'Makes one solid inside a closed string, between two surfaces.', panel: 'cut', mode: 'between', command: 'solid.between_surfaces', step: { takes: 'a closed string, top and bottom' } },
  { key: 'cut:strips', stage: 'solids', label: 'Into strips', tip: 'Slices solids into strips by faces, from the pit out.', panel: 'cut', mode: 'strips', command: 'solid.slice_by_faces', step: { takes: 'solids and a face a strip' } },
  { key: 'cut:cut', stage: 'solids', label: 'By a surface', tip: 'Cuts solids by a surface, a grid or an RL.', panel: 'cut', mode: 'cut', command: 'solid.cut_by_surface', step: { takes: 'solids and a surface, grid or RL' } },
  { key: 'cut:seams', stage: 'solids', label: 'By seams', tip: "Cuts solids by the seam model's horizons, in one run.", panel: 'cut', mode: 'seams', command: 'solid.cut_by_seams', step: { takes: 'solids and a seam model' } },
  { key: 'cut:benches', stage: 'solids', label: 'By benches', tip: 'Cuts pieces at bench levels, by the bench rules.', panel: 'cut', mode: 'benches', command: 'solid.cut_by_benches', step: { takes: 'solids and bench rules' } },
  { key: 'cut:polygons', stage: 'solids', label: 'By polygons', tip: 'Cuts solids vertically into blocks, by named polygons.', panel: 'cut', mode: 'polygons', command: 'solid.cut_by_polygons', step: { takes: 'solids and block polygons' } },
  { key: 'blocks', stage: 'solids', label: 'Block table', tip: 'solids and their attributes to sort, filter, add up, select or delete', icon: 'blocks', panel: 'blocks', short: 'Blocks' },
  { key: 'solids.repair', stage: 'solids', label: 'Repair', tip: 'Fixes closed meshes kept as surfaces because they could not be solids', command: 'solid.repair', action: 'solids.repair' },
  { key: 'import.solids', stage: 'solids', label: 'Import solids', tip: 'A zip of solids, or files of them', command: 'solid.import_many', action: 'import.solids', menu: { in: 'import', at: 4, name: 'A zip or files…', under: SOLIDS_MENU } },
  { key: 'import.solids_folder', stage: 'solids', label: 'Import a folder of solids', tip: 'Every solid in a folder', command: 'solid.import_many', action: 'import.solids_folder', menu: { in: 'import', at: 5, name: 'A folder…', under: SOLIDS_MENU } },
  { key: 'import.solids_repaired', stage: 'solids', label: 'Import solids, repaired', tip: REPAIRED, command: 'solid.import_many', action: 'import.solids_repaired', menu: { in: 'import', at: 6, name: 'A zip or files…', under: SOLIDS_MENU, rule: true, heading: 'Repaired as they come in' } },
  { key: 'import.solids_folder_repaired', stage: 'solids', label: 'Import a folder of solids, repaired', tip: REPAIRED, command: 'solid.import_many', action: 'import.solids_folder_repaired', menu: { in: 'import', at: 7, name: 'A folder…', under: SOLIDS_MENU } },
  { key: 'export.selected_solids', stage: 'solids', label: 'Export selected solids', tip: 'The selected solids as a zip or a folder, a DXF each', command: 'solid.export_many', action: 'export.selected_solids', needs: 'selected-solids', menu: { in: 'export', at: 3, name: 'Selected solids to a zip…' } },
  { key: 'export.visible_solids', stage: 'solids', label: 'Export visible solids', tip: 'Every solid on show as a zip or a folder, a DXF each', command: 'solid.export_many', action: 'export.visible_solids', menu: { in: 'export', at: 6, name: 'Visible solids to a zip…' } },

  // Geology.
  { key: 'import.vulcan_grid', stage: 'geology', label: 'Import seam grids', tip: 'Pick a roof, floor or topography, and the grids of that name come with it', command: 'grid.import_vulcan_seam', action: 'import.vulcan_grid', menu: { in: 'import', at: 2, name: 'Vulcan grid…' } },
  { key: 'seam.suggest', stage: 'geology', label: 'Suggest a stack definition', tip: "Writes a starting definition from the grids' geometry, as a CSV to edit", command: 'seam.suggest_definition', action: 'seam.suggest', menu: { in: 'import', at: 10, name: 'Suggest a stack definition (CSV)…', rule: true, heading: 'Seam model' } },
  { key: 'seam.build', stage: 'geology', label: 'Build a seam model', tip: 'Builds and checks a seam model from a definition', command: 'seam.build_model', action: 'seam.build', menu: { in: 'import', at: 11, name: 'Build and check from a definition…' } },
  { key: 'seam.relink', stage: 'geology', label: 'Relink geology', tip: 'Points a grid folder at its new place', command: 'seam.relink', action: 'seam.relink', menu: { in: 'import', at: 12, name: 'Relink geology…' } },

  // Reserves.
  { key: 'reserves', stage: 'reserves', label: 'Reserves', tip: "interrogate blocks against a seam model, or set them beside Spry's or Deswik's numbers", icon: 'reserves', panel: 'reserves', short: 'Reserves' },
  { key: 'reserves:interrogate', stage: 'reserves', label: 'Interrogate', tip: "Writes each seam grid's value onto blocks, from a seam model.", panel: 'reserves', mode: 'interrogate', command: 'solid.interrogate', step: { takes: 'blocks and a seam model' } },
  { key: 'reserves:compare', stage: 'reserves', label: 'Compare', tip: "Sets blocks beside Spry's or Deswik's numbers.", panel: 'reserves', mode: 'compare', command: 'solid.compare' },
  { key: 'params', stage: 'reserves', label: 'Parameters', tip: 'parameter tables and dataflows edited as grids, a picked solid\'s values marked where they come from', icon: 'parameters', panel: 'params', short: 'Parameters' },
  { key: 'restable', stage: 'reserves', label: 'Reserves table', tip: "a node's fields down the side, broken down by its children across, from Imported to Scheduled", icon: 'reserves-table', panel: 'restable', short: 'Reserves table' },
  { key: 'workflow:report', stage: 'reserves', label: 'Report', tip: "Writes a workflow's report, every case from its latest run", panel: 'workflow' },

  // Always there, whatever the stage.
  { key: 'project.save', stage: 'always', label: 'Save', tip: 'Saves the project', icon: 'save', shortcut: 'Ctrl+S', action: 'project.save', menu: { in: 'project', at: 4, rule: true } },
  { key: 'edit.undo', stage: 'always', label: 'Undo', tip: 'Takes back the last change', icon: 'undo', shortcut: 'Ctrl+Z', action: 'edit.undo', needs: 'undo' },
  { key: 'edit.redo', stage: 'always', label: 'Redo', tip: 'Puts back the change just undone', icon: 'redo', shortcut: 'Ctrl+Y', action: 'edit.redo', needs: 'redo' },
  { key: 'workflow', stage: 'always', label: 'Workflow', tip: 'saved steps run again in one go, captured from the Run log or added from the tool panels', icon: 'workflow', panel: 'workflow', short: 'Workflow' },
  { key: 'canvas', stage: 'always', label: 'Workflow canvas', tip: 'a workflow as nodes and wires: what each step reads', panel: 'canvas' },
  { key: 'layers', stage: 'always', label: 'Layers', tip: 'the layer tree: show, hide, select and arrange', panel: 'layers' },
  { key: 'properties', stage: 'always', label: 'Properties', tip: "what's picked: its name, layer, attributes and measures", panel: 'properties' },
  { key: 'runlog', stage: 'always', label: 'Run log', tip: 'every run, what it made and what it said', panel: 'runlog' },
  { key: 'panels.hide', stage: 'always', label: 'Hide every panel, or bring them back', tip: 'The view fills the window', shortcut: '`', action: 'panels.hide' },
  { key: 'panels.reset', stage: 'always', label: 'Reset layout', tip: 'Puts the panels back where they start', action: 'panels.reset' },
];

export const toolByKey = (key: string): Tool | undefined => TOOLS.find((t) => t.key === key);

/** The tool that opens a panel as it stands: the one keyed by the panel's id. */
export const panelTool = (panel: PanelId): Tool => {
  const tool = TOOLS.find((t) => t.key === panel && t.panel === panel);
  if (!tool) throw new Error(`no tool opens the ${panel} panel`);
  return tool;
};

/** The tool for one of what a panel does. */
export const choiceTool = (panel: PanelId, mode: string): Tool => {
  const tool = TOOLS.find((t) => t.panel === panel && t.mode === mode && t.key === `${panel}:${mode}`);
  if (!tool) throw new Error(`no tool for ${panel} set to ${mode}`);
  return tool;
};

/** A panel's choices as its own choice words them, and the line each puts at the panel's head. */
export function choiceWords<M extends string>(panel: PanelId, modes: readonly M[]): { labels: Record<M, string>; lines: Record<M, string> } {
  const labels = {} as Record<M, string>;
  const lines = {} as Record<M, string>;
  for (const mode of modes) {
    const tool = choiceTool(panel, mode);
    labels[mode] = tool.choice ?? tool.label;
    lines[mode] = tool.tip;
  }
  return { labels, lines };
}

/** A stage's tools, in the order they're listed. */
export const stageTools = (stage: Stage): Tool[] => TOOLS.filter((t) => t.stage === stage);

/** A menu's tools, top to bottom. */
export const menuTools = (menu: MenuId): (Tool & { menu: MenuPlace })[] =>
  TOOLS.filter((t): t is Tool & { menu: MenuPlace } => t.menu?.in === menu).sort((a, b) => a.menu.at - b.menu.at);

/** Today's one row of the toolbar, until it becomes a tab a stage (workstream 5): the panels it opens, left to right. */
export const ONE_ROW: readonly string[] = ['face', 'builder', 'cut', 'reserves', 'blocks', 'restable', 'params', 'workflow'];

/** What a toolbar button's tip says: a panel's name then what it's for, or a tool's name and its keys. */
export const tipOf = (tool: Tool): string => (tool.panel && tool.key === tool.panel ? `${tool.label}: ${tool.tip}` : tool.shortcut ? `${tool.label} (${tool.shortcut})` : tool.tip);

/** A menu item's words, with how many it would take where that's more than one ("Selected to DXF (3)…"). */
export function menuWords(tool: Tool, count: number | null): string {
  const name = tool.menu?.name ?? tool.label;
  if (count === null || count <= 1) return name;
  return name.endsWith('…') ? `${name.slice(0, -1)} (${count})…` : `${name} (${count})`;
}

/** The tools that can add a step to a workflow, for the canvas. */
export const stepTools = (): (Tool & { panel: PanelId; command: string; step: { takes: string } })[] =>
  TOOLS.filter((t): t is Tool & { panel: PanelId; command: string; step: { takes: string } } => !!t.step && !!t.panel && !!t.command);
